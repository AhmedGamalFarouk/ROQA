//! Persistence store (M5 S2): one small versioned file, read once at startup and written only on change.
//!
//! Format is a hand-rolled `key=value` text file (no new crate): `version=N`, repeated `pin=`/`shelf=` lines,
//! the settings lines (`settings.rs`, version 2), and a closing `end` line that makes truncation detectable.
//! Unknown keys are skipped, so a newer build's extra fields survive an older reader; a *newer version* is set
//! aside instead, so a downgrade never rewrites it.

use crate::settings::Settings;
use crate::tabs::Tab;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const VERSION: u32 = 2;
/// Bigger files are not read at all; saves refuse to exceed it.
pub const MAX_BYTES: u64 = 64 * 1024;
/// A burst of changes becomes one write this long after the last change (UNVERIFIED value; tune in S9).
pub const COALESCE: Duration = Duration::from_millis(1500);
/// A never-ending burst still writes this long after its first unwritten change (UNVERIFIED value).
pub const MAX_WAIT: Duration = Duration::from_secs(10);
/// A failed write is retried this long after it, so a full or locked disk costs one wake per 30 s at most.
pub const RETRY: Duration = Duration::from_secs(30);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Store {
    /// Pinned apps (AUMID or executable path), in display order.
    pub pins: Vec<String>,
    /// File-shelf references, in display order.
    pub shelf: Vec<String>,
    /// Running countdown timers as absolute wall-clock expiries (Unix seconds), soonest first.
    pub timers: Vec<u64>,
    /// User settings; version 1 files had none, so they load as the defaults.
    pub settings: Settings,
    /// The notch tab used last (a state, not a setting: the settings window must not write it back).
    pub last_tab: Tab,
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

/// As `flush_decision`, but a burst cannot postpone the write past `MAX_WAIT` after its first change.
pub fn flush_decision_capped(first_change: Instant, last_change: Instant, now: Instant) -> Flush {
    match flush_decision(true, last_change, now) {
        Flush::Wait(_) if now >= first_change + MAX_WAIT => Flush::Now,
        Flush::Wait(due) => Flush::Wait(due.min(first_change + MAX_WAIT)),
        other => other,
    }
}

/// Unwritten changes: when the first and the latest happened. Clean is `None`, which has no deadline.
#[derive(Debug, Default)]
pub struct Pending(Option<(Instant, Instant)>);

impl Pending {
    pub fn mark(&mut self, now: Instant) {
        self.0 = Some((self.0.map_or(now, |(first, _)| first), now));
    }
    pub fn decision(&self, now: Instant) -> Flush {
        self.0.map_or(Flush::Idle, |(first, last)| {
            flush_decision_capped(first, last, now)
        })
    }
    /// Write now? Quit writes anything unwritten at once; otherwise only a due flush does.
    pub fn should_write(&self, now: Instant, quit: bool) -> bool {
        (quit && self.0.is_some()) || self.decision(now) == Flush::Now
    }
    pub fn written(&mut self) {
        self.0 = None;
    }
    /// A failed write stays pending and is next due `RETRY` after `now`.
    pub fn retry(&mut self, now: Instant) {
        let at = now + RETRY - COALESCE;
        self.0 = Some((at, at));
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
            Some(("timer", value)) => store.timers.extend(value.parse::<u64>()),
            Some(("lasttab", value)) => {
                store.last_tab = Tab::from_key(value)
                    .filter(|tab| tab.available())
                    .unwrap_or_default();
            }
            Some((key, value)) if store.settings.set(key, value) => {}
            _ => {} // Unknown keys and junk lines are ignored.
        }
    }
    if !ended {
        return None;
    }
    migrate(version, &mut store);
    Some(store)
}

/// Brings an older file's contents to the current schema; a bump adds an `if from < N { ... }` step.
fn migrate(from: u32, store: &mut Store) {
    if from < 2 {
        // v1 had no settings section: whatever a stray line said, a v1 file means the defaults.
        store.settings = Settings::default();
    }
    store.settings = std::mem::take(&mut store.settings).clamped();
}

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
        for expiry in &self.timers {
            out.push_str(&format!("timer={expiry}\n"));
        }
        out.push_str(&format!("lasttab={}\n", self.last_tab.key()));
        self.settings.write_lines(&mut out);
        out.push_str("end\n");
        out
    }

    /// `save` plus the diagnostics write counter; the host's flush path, so unit-test saves are not counted.
    pub fn flush(&self, path: &Path) -> io::Result<()> {
        self.save(path)
            .inspect(|()| crate::diagnostics::note_store_write())
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
            timers: vec![1_800_000_000, 1_800_000_060],
            settings: Settings {
                dock: false,
                off_monitors: vec![r"\\.\DISPLAY2".into()],
                timer_sound: false,
                shelf_cap: 6,
                diagnostics: Some(r"C:\logs\run=1.json".into()),
                // A tab switched off and the mascot off survive a restart.
                tabs: [true, true, true, false, true],
                eyes: false,
                ..Settings::default()
            },
            last_tab: Tab::Shelf,
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
            ..Store::default()
        };
        store.save(&dir.file()).unwrap();
        assert_eq!(load(&dir.file()).0.pins, vec!["ok".to_string()]);
    }

    #[test]
    fn unknown_fields_and_junk_lines_are_ignored() {
        let text = "version=1\nfuture=1\npin=a\nno equals here\nshelf=b\ntimer=77\ntimer=x\ntimer=-1\nfuture2=x=y\nend\nafter end=ignored\n";
        assert_eq!(
            parse(text),
            Some(Store {
                pins: vec!["a".into()],
                shelf: vec!["b".into()],
                timers: vec![77], // unparsable timer values are skipped
                settings: Settings::default(),
                last_tab: Tab::Media,
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
        // Only the settings differ: version 1 had none, so they reset to the defaults.
        assert_eq!(
            store,
            Store {
                settings: Settings::default(),
                ..sample()
            }
        );
        migrate(2, &mut store);
        assert_eq!(
            store,
            Store {
                settings: Settings::default(),
                ..sample()
            }
        );
        let mut v2 = sample();
        migrate(2, &mut v2);
        assert_eq!(v2, sample(), "a current-version file keeps its settings");
    }

    #[test]
    fn a_version_1_file_migrates_to_default_settings_and_a_v2_one_is_clamped() {
        let v1 = parse("version=1\npin=a\ndock=0\nshelfcap=2\nend\n").unwrap();
        assert_eq!(
            (v1.pins, v1.settings),
            (vec!["a".to_string()], Settings::default())
        );
        let v2 = parse(
            "version=2\ndock=0\nnotch=0\nshelfcap=999\nsound=0\noff=D2\ndiag= \"C:\\d.json\" \nend\n",
        )
        .unwrap();
        assert_eq!(
            v2.settings,
            Settings {
                dock: false,
                notch: true, // at least one kind stays on
                timer_sound: false,
                shelf_cap: crate::shelf::MAX,
                off_monitors: vec!["D2".into()],
                diagnostics: Some(r"C:\d.json".into()),
                ..Settings::default()
            }
        );
        // No settings lines in a v2 file means the defaults too.
        assert_eq!(
            parse("version=2\nend\n").unwrap().settings,
            Settings::default()
        );
        let dir = Dir::new("v1");
        std::fs::write(dir.file(), "version=1\nshelf=C:\\a\nend\n").unwrap();
        let (loaded, state) = load(&dir.file());
        assert_eq!(
            (state, loaded.settings.clone()),
            (Load::Loaded, Settings::default())
        );
        loaded.save(&dir.file()).unwrap();
        assert!(
            std::fs::read_to_string(dir.file())
                .unwrap()
                .starts_with("version=2\n")
        );
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
            ..Store::default()
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

    #[test]
    fn a_never_ending_burst_still_writes_by_the_max_wait() {
        let t0 = Instant::now();
        let mut pending = Pending::default();
        assert_eq!(pending.decision(t0), Flush::Idle);
        assert!(!pending.should_write(t0, true)); // clean: nothing to write at quit either
        // A change every second for a minute: each restarts the coalesce wait, so only the cap fires.
        let mut writes = Vec::new();
        for second in 0..60u64 {
            let now = t0 + Duration::from_secs(second);
            pending.mark(now);
            if pending.should_write(now, false) {
                writes.push(second);
                pending.written();
            }
        }
        assert_eq!(writes, [10, 21, 32, 43, 54]); // first change after a write starts a new cap window
        assert_eq!(
            flush_decision_capped(t0, t0 + Duration::from_secs(9), t0 + Duration::from_secs(9)),
            Flush::Wait(t0 + MAX_WAIT)
        );
        assert_eq!(
            flush_decision_capped(t0, t0, t0),
            Flush::Wait(t0 + COALESCE)
        );
    }

    #[test]
    fn pending_flushes_once_quit_writes_dirty_and_failure_retries_later() {
        let t0 = Instant::now();
        let mut pending = Pending::default();
        pending.mark(t0);
        pending.mark(t0 + Duration::from_millis(500));
        assert_eq!(
            pending.decision(t0 + Duration::from_millis(600)),
            Flush::Wait(t0 + Duration::from_millis(2000))
        );
        assert!(!pending.should_write(t0 + Duration::from_millis(1999), false));
        assert!(pending.should_write(t0 + Duration::from_millis(1999), true)); // quit ignores the delay
        assert!(pending.should_write(t0 + Duration::from_millis(2000), false));
        // A failed write leaves it pending and due RETRY later, not at the next coalesce tick.
        let failed = t0 + Duration::from_secs(5);
        pending.retry(failed);
        assert_eq!(pending.decision(failed), Flush::Wait(failed + RETRY));
        assert!(pending.should_write(failed + RETRY, false));
        pending.written();
        assert_eq!(pending.decision(failed + RETRY * 9), Flush::Idle);
    }
}
