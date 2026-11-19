//! The dock's Places button: the Recycle Bin and each fixed or removable drive, read when the popup opens.
//! Network drives are left out (a stat on one can hang the host thread) and nothing is kept or polled.
//! A pinned folder's stack (its newest files) is read here too, once per click.
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW},
        System::Com::CoTaskMemFree,
        UI::Shell::{
            FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath, SHQUERYRBINFO,
            SHQueryRecycleBinW,
        },
    },
    core::{HSTRING, PCWSTR},
};

/// How many files a folder stack lists.
pub const STACK: usize = 8;

/// The user's Downloads folder, as the shell knows it (it may be moved off the profile).
pub fn downloads() -> Option<String> {
    // SAFETY: the shell returns a COM string that is copied here and then freed once.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0.cast()));
        text
    }
}

/// The newest `count` files of `dir` (folders, hidden files and `desktop.ini` left out), newest first.
pub fn newest_files(dir: &Path, count: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !meta.is_file() || name.starts_with('.') || name == "desktop.ini" {
                return None;
            }
            Some((meta.modified().ok()?, entry.path()))
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files
        .into_iter()
        .take(count)
        .map(|(_, path)| path)
        .collect()
}

/// `GetDriveTypeW` answers for a removable and a fixed drive.
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;

/// One popup row: its text and what the shell opens.
pub struct Place {
    pub text: String,
    pub target: String,
}

/// The Recycle Bin's row text: how many items it holds.
fn bin_text(items: i64) -> String {
    match items {
        0 => "Recycle Bin, empty".into(),
        1 => "Recycle Bin, 1 item".into(),
        n => format!("Recycle Bin, {n} items"),
    }
}

/// A drive's row text: its label (or "Drive") and letter.
fn drive_text(label: &str, letter: char) -> String {
    let name = if label.is_empty() { "Drive" } else { label };
    format!("{name} ({letter}:)")
}

pub fn list() -> Vec<Place> {
    let mut info = SHQUERYRBINFO {
        cbSize: size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a live, sized out value; a null root asks for every drive's bin.
    let items =
        unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }.map_or(0, |()| info.i64NumItems);
    let mut places = vec![Place {
        text: bin_text(items),
        target: "shell:RecycleBinFolder".into(),
    }];
    // SAFETY: a scalar query of the logical drive bitmask.
    let mask = unsafe { GetLogicalDrives() };
    for index in 0..26u8 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let letter = char::from(b'A' + index);
        let root = HSTRING::from(format!("{letter}:\\"));
        // SAFETY: `root` outlives the call.
        let kind = unsafe { GetDriveTypeW(&root) };
        if kind != DRIVE_FIXED && kind != DRIVE_REMOVABLE {
            continue;
        }
        let mut name = [0u16; 64];
        // SAFETY: `root` and the bounded buffer outlive the call; an empty card reader just fails.
        let read = unsafe { GetVolumeInformationW(&root, Some(&mut name), None, None, None, None) };
        if kind == DRIVE_REMOVABLE && read.is_err() {
            continue; // nothing inserted
        }
        let label =
            String::from_utf16_lossy(&name[..name.iter().position(|c| *c == 0).unwrap_or(0)]);
        places.push(Place {
            text: drive_text(&label, letter),
            target: root.to_string(),
        });
    }
    places
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rows_read_plainly() {
        assert_eq!(bin_text(0), "Recycle Bin, empty");
        assert_eq!(bin_text(1), "Recycle Bin, 1 item");
        assert_eq!(bin_text(12), "Recycle Bin, 12 items");
        assert_eq!(drive_text("", 'C'), "Drive (C:)");
        assert_eq!(drive_text("Data", 'D'), "Data (D:)");
    }
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "a test writes a temp file, never the store"
    )]
    fn a_stack_lists_the_newest_files_only() {
        let dir = std::env::temp_dir().join(format!("roqa-stack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a folder")).unwrap();
        let start = std::time::SystemTime::now() - std::time::Duration::from_secs(1000);
        for (n, name) in ["old.txt", "mid.txt", "new.txt", "desktop.ini", ".hidden"]
            .into_iter()
            .enumerate()
        {
            let file = std::fs::File::create(dir.join(name)).unwrap();
            file.set_modified(start + std::time::Duration::from_secs(n as u64 * 10))
                .unwrap();
        }
        let names = |count| -> Vec<_> {
            newest_files(&dir, count)
                .iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(names(8), ["new.txt", "mid.txt", "old.txt"]);
        assert_eq!(names(2), ["new.txt", "mid.txt"]);
        assert!(newest_files(&dir.join("missing"), 8).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn the_bin_comes_first() {
        assert_eq!(list()[0].target, "shell:RecycleBinFolder");
    }
}
