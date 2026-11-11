//! Persistence store (M5 S2): one small versioned file, read once at startup and written only on change.
//!
//! Format is a hand-rolled `key=value` text file (no new crate): `version=N`, repeated `pin=`/`shelf=` lines,
//! and a closing `end` line that makes truncation detectable. Unknown keys are skipped, so a newer build's
//! extra fields survive an older reader; a *newer version* is set aside instead, so a downgrade never rewrites it.
#![allow(dead_code)] // S3, S4 and S6 wire the host; until then only the tests call this module.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const VERSION: u32 = 1;
/// Bigger files are not read at all; saves refuse to exceed it.
pub const MAX_BYTES: u64 = 64 * 1024;
/// A burst of changes becomes one write this long after the last change (UNVERIFIED value; tune in S9).
pub const COALESCE: Duration = Duration::from_millis(1500);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Store {
    /// Pinned apps (AUMID or executable path), in display order.
    pub pins: Vec<String>,
    /// File-shelf references, in display order.
    pub shelf: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Load {
    Missing,
    Loaded,
    /// Corrupt, truncated, oversized or newer-version: defaults used, the file renamed to `*.bad`.
    Quarantined,
    /// Present but unreadable (e.g. locked): defaults used, the file left alone.
    Unreadable,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Flush {
    Idle,
    /// Not due yet; the host deadline is this instant.
    Wait(Instant),
    Now,
}

/// `%LOCALAPPDATA%\ROQA\store.roqa`.
pub fn default_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("ROQA").join("store.roqa"))
}

/// Pure: write once the last change is `COALESCE` old. At quit the host writes iff dirty, ignoring the delay.
pub fn flush_decision(dirty: bool, last_change: Instant, now: Instant) -> Flush {
    if !dirty {
        return Flush::Idle;
    }
    let due = last_change + COALESCE;
    if now >= due {
        Flush::Now
    } else {
        Flush::Wait(due)
    }
}

/// Reads the file once and closes it. Never fails: any problem yields defaults.
pub fn load(path: &Path) -> (Store, Load) {
    let bytes = match std::fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_BYTES => None,
        Ok(_) => match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return (Store::default(), Load::Missing);
            }
            Err(_) => return (Store::default(), Load::Unreadable),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (Store::default(), Load::Missing),
        Err(_) => return (Store::default(), Load::Unreadable),
    };
    let parsed = bytes
        .filter(|bytes| bytes.len() as u64 <= MAX_BYTES)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| parse(&text));
    match parsed {
        Some(store) => (store, Load::Loaded),
        None => {
            // Keep one bad copy for the user; a failed rename just means the next save replaces it.
            let _ = std::fs::rename(path, path.with_extension("roqa.bad"));
            (Store::default(), Load::Quarantined)
        }
    }
}

fn parse(text: &str) -> Option<Store> {
    let mut lines = text.lines();
    let version: u32 = lines
        .next()?
        .strip_prefix("version=")?
        .trim()
        .parse()
        .ok()?;
    if version == 0 || version > VERSION {
        return None;
    }
    let mut store = Store::default();
    let mut ended = false;
    for line in lines {
        if line == "end" {
            ended = true;
            break;
        }
        match line.split_once('=') {
            Some(("pin", value)) if !value.is_empty() => store.pins.push(value.into()),
            Some(("shelf", value)) if !value.is_empty() => store.shelf.push(value.into()),
            _ => {} // Unknown keys and junk lines are ignored.
        }
    }
    if !ended {
        return None;
    }
    migrate(version, &mut store);
    Some(store)
}

/// Migration stub: VERSION is 1, so nothing runs yet. A bump adds `if from < N { ... }` steps here.
fn migrate(_from: u32, _store: &mut Store) {}

impl Store {
    fn serialize(&self) -> String {
        let mut out = format!("version={VERSION}\n");
        for (key, list) in [("pin", &self.pins), ("shelf", &self.shelf)] {
            // Newlines cannot be represented in this line format (and cannot occur in Windows paths or AUMIDs).
            for value in list
                .iter()
                .filter(|v| !v.is_empty() && !v.contains(['\n', '\r']))
            {
                out.push_str(key);
                out.push('=');
                out.push_str(value);
                out.push('\n');
            }
        }
        out.push_str("end\n");
        out
    }

    /// Atomic temp-and-rename write, same pattern as the opt-in diagnostics. A stale temp file from a crashed
    /// process (a different pid) is ignored by `load` and left in place: at most one tiny file per crash.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = self.serialize();
        if text.len() as u64 > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "store exceeds its size cap",
            ));
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temporary = path.with_extension(format!("roqa-{}.tmp", std::process::id()));
        std::fs::write(&temporary, text)
            .and_then(|()| std::fs::rename(&temporary, path))
            .inspect_err(|_| {
                let _ = std::fs::remove_file(&temporary);
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory under the system temp dir, removed on drop. Never touches `%LOCALAPPDATA%`.
    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("roqa-store-test-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }
        fn file(&self) -> PathBuf {
            self.0.join("store.roqa")
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sample() -> Store {
        Store {
            pins: vec![
                "Microsoft.Edge_8wekyb3d8bbwe!App".into(),
                r"C:\Apps\a=b.exe".into(),
            ],
            shelf: vec![r"C:\Users\me\doc.txt".into()],
        }
    }

    #[test]
    fn round_trip_and_missing() {
        let dir = Dir::new("round");
        assert_eq!(load(&dir.file()), (Store::default(), Load::Missing));
        sample().save(&dir.file()).unwrap();
        assert_eq!(load(&dir.file()), (sample(), Load::Loaded));
        // Order and an empty store both survive.
        Store::default().save(&dir.file()).unwrap();
        assert_eq!(load(&dir.file()), (Store::default(), Load::Loaded));
    }

    #[test]
    fn unrepresentable_values_are_dropped_not_corrupting() {
        let dir = Dir::new("newline");
        let store = Store {
            pins: vec!["a\nb".into(), "ok".into(), String::new()],
            shelf: vec![],
        };
        store.save(&dir.file()).unwrap();
        assert_eq!(load(&dir.file()).0.pins, vec!["ok".to_string()]);
    }

    #[test]
    fn unknown_fields_and_junk_lines_are_ignored() {
        let text = "version=1\nfuture=1\npin=a\nno equals here\nshelf=b\nfuture2=x=y\nend\nafter end=ignored\n";
        assert_eq!(
            parse(text),
            Some(Store {
                pins: vec!["a".into()],
                shelf: vec!["b".into()]
            })
        );
        assert_eq!(
            parse("version=1\r\npin=a\r\nend\r\n").unwrap().pins,
            vec!["a".to_string()]
        );
    }

    #[test]
    fn version_handling() {
        assert!(parse("version=1\nend\n").is_some());
        assert!(parse("version=0\nend\n").is_none());
        assert!(parse(&format!("version={}\nend\n", VERSION + 1)).is_none());
        assert!(parse("version=abc\nend\n").is_none());
        assert!(parse("pin=a\nversion=1\nend\n").is_none()); // version must be first
        let dir = Dir::new("newer");
        std::fs::write(
            dir.file(),
            format!("version={}\npin=keep\nend\n", VERSION + 1),
        )
        .unwrap();
        assert_eq!(load(&dir.file()), (Store::default(), Load::Quarantined));
        // The newer file is preserved aside, never overwritten.
        let kept = std::fs::read_to_string(dir.0.join("store.roqa.bad")).unwrap();
        assert!(kept.contains("pin=keep"));
        let mut store = sample();
        migrate(1, &mut store);
        assert_eq!(store, sample());
    }

    #[test]
    fn corrupt_empty_truncated_and_non_utf8_load_defaults_and_move_aside() {
        let cases: [&[u8]; 4] = [
            b"",
            b"garbage\x00\x01",
            b"version=1\npin=C:\\half\\writ",
            b"version=1\npin=\xff\xfe\nend\n",
        ];
        for (index, bytes) in cases.iter().enumerate() {
            let dir = Dir::new(&format!("bad{index}"));
            std::fs::write(dir.file(), bytes).unwrap();
            assert_eq!(
                load(&dir.file()),
                (Store::default(), Load::Quarantined),
                "case {index}"
            );
            assert!(
                !dir.file().exists() && dir.0.join("store.roqa.bad").exists(),
                "case {index}"
            );
            // The next save works normally and a second quarantine replaces the old .bad file.
            sample().save(&dir.file()).unwrap();
            assert_eq!(load(&dir.file()).1, Load::Loaded);
        }
    }

    #[test]
    fn oversized_file_is_not_read_and_moved_aside() {
        let dir = Dir::new("big");
        let body = format!("version=1\npin={}\nend\n", "x".repeat(MAX_BYTES as usize));
        std::fs::write(dir.file(), body).unwrap();
        assert_eq!(load(&dir.file()), (Store::default(), Load::Quarantined));
        assert!(dir.0.join("store.roqa.bad").exists());
    }

    #[test]
    fn save_refuses_over_the_cap_and_leaves_the_old_file() {
        let dir = Dir::new("cap");
        sample().save(&dir.file()).unwrap();
        let huge = Store {
            pins: vec!["y".repeat(MAX_BYTES as usize)],
            shelf: vec![],
        };
        assert_eq!(
            huge.save(&dir.file()).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(load(&dir.file()), (sample(), Load::Loaded));
    }

    #[test]
    fn atomic_write_leaves_no_temp_and_ignores_a_stale_one() {
        let dir = Dir::new("atomic");
        // A leftover temp file from a crashed writer must not affect loading or saving.
        let stale = dir.0.join("store.roqa-99999.tmp");
        std::fs::write(&stale, "version=1\npin=stale\n").unwrap();
        assert_eq!(load(&dir.file()).1, Load::Missing);
        sample().save(&dir.file()).unwrap();
        let mut names: Vec<_> = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["store.roqa", "store.roqa-99999.tmp"]); // our own temp was renamed away
        assert_eq!(load(&dir.file()).0, sample());
        // A failed rename (target is a directory) cleans up its temp file and reports the error.
        let blocked = Dir::new("blocked");
        std::fs::create_dir(blocked.file()).unwrap();
        assert!(sample().save(&blocked.file()).is_err());
        assert_eq!(std::fs::read_dir(&blocked.0).unwrap().count(), 1);
    }

    #[test]
    fn save_creates_the_directory() {
        let dir = Dir::new("mkdir");
        let nested = dir.0.join("ROQA").join("store.roqa");
        sample().save(&nested).unwrap();
        assert_eq!(load(&nested).0, sample());
    }

    #[test]
    fn flush_decision_is_pure_and_coalesces() {
        let t0 = Instant::now();
        assert_eq!(flush_decision(false, t0, t0 + COALESCE * 9), Flush::Idle);
        assert_eq!(flush_decision(true, t0, t0), Flush::Wait(t0 + COALESCE));
        // N changes inside the delay: each restarts the wait and none is due, so no write happens until it passes.
        for step in 1..5u32 {
            let last = t0 + Duration::from_millis(100) * step;
            let now = last + COALESCE - Duration::from_millis(1);
            assert_eq!(
                flush_decision(true, last, now),
                Flush::Wait(last + COALESCE)
            );
        }
        let last = t0 + Duration::from_millis(400);
        assert_eq!(flush_decision(true, last, last + COALESCE), Flush::Now);
        assert_eq!(flush_decision(true, last, last + COALESCE * 3), Flush::Now);
    }
}
