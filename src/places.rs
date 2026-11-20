//! The dock's Places button: the Recycle Bin and each fixed or removable drive, read when the popup opens, on a short worker
//! (`ask_places`), since a sleeping drive can stall the call.
//! Network drives are left out (a stat on one can hang the host thread) and nothing is kept or polled.
//! A pinned folder's stack (its newest files) is read here too, once per click, on the same worker (`ask`).
use std::path::{Path, PathBuf};
use std::sync::PoisonError;
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

/// The newest `count` files of `dir` (folders, hidden files and `desktop.ini` left out), newest first. Only the best
/// `count` are kept while the folder is read, so a huge folder costs one pass and no sorted copy.
pub fn newest_files(dir: &Path, count: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut best = Vec::with_capacity(count + 1);
    for entry in entries.filter_map(Result::ok) {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !meta.is_file() || name.starts_with('.') || name == "desktop.ini" {
            continue;
        }
        if let Ok(modified) = meta.modified() {
            keep_newest(&mut best, (modified, entry.path()), count);
        }
    }
    best.into_iter().map(|(_, path)| path).collect()
}

/// Puts `item` into `best` (newest first, at most `count`) unless `count` newer ones are already there; among equals
/// the earlier one stays ahead.
fn keep_newest<T: Ord, P>(best: &mut Vec<(T, P)>, item: (T, P), count: usize) {
    let at = best.partition_point(|(time, _)| *time >= item.0);
    if at < count {
        best.insert(at, item);
        best.truncate(count);
    }
}

/// Whether a pinned path may be a folder: a program or a shortcut is not, and needs no look at the disk to say so.
pub fn may_be_folder(path: &str) -> bool {
    const PROGRAMS: [&str; 7] = ["exe", "lnk", "bat", "cmd", "com", "msi", "url"];
    !Path::new(path)
        .extension()
        .is_some_and(|ext| PROGRAMS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
}

/// What a click on a pinned path found: its newest files when it is a folder, `None` when it is not.
pub struct Answer {
    /// The dock window the click came from (a handle value), and the slot.
    pub dock: isize,
    pub slot: usize,
    pub folder: String,
    pub files: Option<Vec<PathBuf>>,
    /// The Places popup's rows, for a question asked with `ask_places`.
    pub places: Option<Vec<Place>>,
    request: u64,
    asked: std::time::Instant,
}

/// An answer older than this is dropped: a stack that pops up long after the click is not what was asked for.
pub const STALE: std::time::Duration = std::time::Duration::from_secs(2);

impl Answer {
    fn latest(&self) -> bool {
        self.request == REQUEST.load(std::sync::atomic::Ordering::Acquire)
    }
    /// Whether the click that asked is still the latest one, with no menu opened since, and recent.
    pub fn current(&self) -> bool {
        self.latest() && self.asked.elapsed() <= STALE
    }
    /// Whether the click that asked is still the latest but the read took too long to use.
    pub fn slow(&self) -> bool {
        self.latest() && self.asked.elapsed() > STALE
    }
}

/// Posted to the host when `ask` has an answer; the host reads it with `answer` on every pass.
pub const READY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 62;
/// The question out: its request number and when it was asked. A newer question may start once this one is older
/// than `STALE` (a stuck share must not block the popup for ever); only the latest question's answer is kept.
static ASKING: std::sync::Mutex<Option<(u64, std::time::Instant)>> = std::sync::Mutex::new(None);
static ANSWER: std::sync::Mutex<Option<Answer>> = std::sync::Mutex::new(None);
/// The generation of the latest click; opening any menu (`invalidate`) makes an older answer not current.
static REQUEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Workers still inside a blocking OS call, expired ones included (`ASKING` only forgets them). A stalled share must not
/// pile up threads on retries, so `start` refuses past `MAX_LIVE`.
static LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
const MAX_LIVE: usize = 3;

/// One admitted worker; dropping it (the worker ends, or never started) frees its place.
struct Live(&'static std::sync::atomic::AtomicUsize);

impl Live {
    fn admit(count: &'static std::sync::atomic::AtomicUsize, max: usize) -> Option<Self> {
        use std::sync::atomic::Ordering::AcqRel;
        if count.fetch_add(1, AcqRel) >= max {
            count.fetch_sub(1, AcqRel);
            return None;
        }
        Some(Self(count))
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// A menu opened: a question still out no longer applies.
pub fn invalidate() {
    REQUEST.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
}

/// Asks a short worker whether `folder` is a folder and what its newest files are: a stat on a slow or offline
/// share must not stop the host. One question at a time; false when one is still out or no worker could start.
pub fn ask(dock: isize, slot: usize, folder: String) -> bool {
    start(dock, slot, folder, false)
}

/// Asks the same worker for the Places rows (`list`): a sleeping drive must not stop the host either.
pub fn ask_places(dock: isize) -> bool {
    start(dock, 0, String::new(), true)
}

/// Whether a question asked at `asked` (if any) still holds the worker at `now`.
fn holds(asked: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    asked.is_some_and(|asked| now.saturating_duration_since(asked) <= STALE)
}

fn start(dock: isize, slot: usize, folder: String, listing: bool) -> bool {
    use std::sync::atomic::Ordering::AcqRel;
    let now = std::time::Instant::now();
    let mut asking = ASKING.lock().unwrap_or_else(PoisonError::into_inner);
    if holds(asking.map(|(_, asked)| asked), now) {
        return false;
    }
    let Some(live) = Live::admit(&LIVE, MAX_LIVE) else {
        return false;
    };
    // SAFETY: a scalar thread id query.
    let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let (request, asked) = (REQUEST.fetch_add(1, AcqRel) + 1, now);
    *asking = Some((request, asked));
    let spawned = std::thread::Builder::new()
        .name("roqa-stack".into())
        .spawn(move || {
            let _live = live;
            let path = Path::new(&folder);
            let files = (!listing && path.is_dir()).then(|| newest_files(path, STACK));
            let places = listing.then(list);
            // A question that was passed over while it ran (a newer one started) never replaces the newer's answer.
            let asking = ASKING.lock().unwrap_or_else(PoisonError::into_inner);
            if asking.is_some_and(|(latest, _)| latest == request)
                && let Ok(mut slot_answer) = ANSWER.lock()
            {
                *slot_answer = Some(Answer {
                    dock,
                    slot,
                    folder,
                    files,
                    places,
                    request,
                    asked,
                });
                drop(asking);
                crate::native_window::post_wake(host, READY);
            }
        });
    if spawned.is_err() {
        *asking = None;
    }
    spawned.is_ok()
}

/// The finished question's answer, once; the next question may start after it.
pub fn answer() -> Option<Answer> {
    let answer = ANSWER.lock().ok()?.take()?;
    // A newer question may have started since this answer was published: it keeps its place.
    let mut asking = ASKING.lock().unwrap_or_else(PoisonError::into_inner);
    if asking.is_some_and(|(latest, _)| latest == answer.request) {
        *asking = None;
    }
    Some(answer)
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
    fn a_late_or_overtaken_answer_is_not_current() {
        let answer = |request, asked| Answer {
            dock: 0,
            slot: 0,
            folder: String::new(),
            files: None,
            places: None,
            request,
            asked,
        };
        let request = REQUEST.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
        let now = std::time::Instant::now();
        assert!(answer(request, now).current());
        let old = now.checked_sub(STALE * 2).unwrap_or(now);
        assert!(!answer(request, old).current(), "too slow");
        assert!(answer(request, old).slow() && !answer(request, now).slow());
        invalidate();
        assert!(!answer(request, now).current(), "a menu opened since");
        assert!(
            !answer(request, old).slow(),
            "a later click owns the answer"
        );
    }
    #[test]
    fn a_question_holds_the_worker_only_until_it_is_stale() {
        let now = std::time::Instant::now();
        assert!(!holds(None, now));
        assert!(holds(Some(now), now));
        assert!(holds(Some(now), now + STALE));
        assert!(!holds(
            Some(now),
            now + STALE + std::time::Duration::from_millis(1)
        ));
    }
    #[test]
    fn consuming_an_old_answer_keeps_the_newer_question() {
        let now = std::time::Instant::now();
        let old = Answer {
            dock: 0,
            slot: 0,
            folder: String::new(),
            files: None,
            places: None,
            request: 4,
            asked: now,
        };
        *ASKING.lock().unwrap() = Some((5, now));
        *ANSWER.lock().unwrap() = Some(old);
        assert!(answer().is_some());
        assert_eq!(ASKING.lock().unwrap().map(|(request, _)| request), Some(5));
        *ASKING.lock().unwrap() = None;
    }
    #[test]
    fn stalled_workers_are_bounded_and_free_their_place() {
        static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let mut held: Vec<_> = (0..2).filter_map(|_| Live::admit(&COUNT, 2)).collect();
        assert_eq!(held.len(), 2);
        assert!(Live::admit(&COUNT, 2).is_none(), "a third would pile up");
        held.pop();
        assert!(
            Live::admit(&COUNT, 2).is_some(),
            "a finished one frees its place"
        );
    }
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
    fn only_the_newest_few_are_kept_while_reading() {
        let mut best = Vec::new();
        for (time, name) in [(5, "e"), (9, "i"), (1, "a"), (9, "j"), (7, "g"), (3, "c")] {
            keep_newest(&mut best, (time, name), 3);
            assert!(best.len() <= 3);
        }
        // Newest first; of two equal times the earlier read stays ahead.
        assert_eq!(best, [(9, "i"), (9, "j"), (7, "g")]);
        keep_newest(&mut best, (1, "z"), 3);
        assert_eq!(best.len(), 3);
    }
    #[test]
    fn a_program_or_shortcut_is_not_asked_about_as_a_folder() {
        for program in ["a.EXE", r"C:\x\b.lnk", "run.cmd"] {
            assert!(!may_be_folder(program), "{program}");
        }
        for folder in [r"C:\Users\me\Downloads", r"\nas\share", r"D:\v2.1"] {
            assert!(may_be_folder(folder), "{folder}");
        }
    }
    #[test]
    fn the_bin_comes_first() {
        assert_eq!(list()[0].target, "shell:RecycleBinFolder");
    }
}
