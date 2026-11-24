//! Persistence store (M5 S2): one small versioned file, read once at startup and written only on change.
//!
//! Format is a hand-rolled `key=value` text file (no new crate): `version=N`, repeated `pin=`/`shelf=` lines,
//! the settings lines (`settings.rs`, version 2), and a closing `end` line that makes truncation detectable.
//! Unknown keys are skipped when reading and are dropped at the next save (only known fields are written); a
//! *newer version* is left alone and not saved over, so a downgrade never rewrites it.

use crate::settings::Settings;
use crate::tabs::Tab;
use crate::weather::{Place, Report};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;
use std::time::{Duration, Instant};

pub const VERSION: u32 = 2;
/// Bigger files are never parsed (a newer build's head is still checked); saves refuse to exceed it.
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
    pub folders: Vec<String>,
    /// Apps hidden from the dock (same keys as pins).
    pub hidden: Vec<String>,
    /// File-shelf references, in display order.
    pub shelf: Vec<String>,
    /// Running countdown timers as absolute wall-clock expiries (Unix seconds), soonest first.
    pub timers: Vec<u64>,
    /// Reminders waiting (`remind.rs`): the Unix-second expiry and the label, soonest first.
    pub reminders: Vec<(u64, String)>,
    /// User settings; version 1 files had none, so they load as the defaults.
    pub settings: Settings,
    /// The notch tab used last (a state, not a setting: the settings window must not write it back).
    pub last_tab: Tab,
    /// The weather place and the last report (features W1): shown at once on the next start.
    pub place: Option<Place>,
    pub report: Option<Report>,
    /// Water progress: the local day (`prayer_tab::day_number`) and the glasses drunk on it.
    pub water: (i64, u8),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Load {
    Missing,
    Loaded,
    /// Corrupt, truncated or oversized: defaults used, the file renamed to `*.bad` (never over an earlier one).
    Quarantined,
    /// Present but unreadable (e.g. locked), from a newer version or not quarantinable: defaults used, the file left alone.
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

/// Unwritten changes: when the first and the latest happened (clean is `None`, which has no deadline), whether one
/// came since `take_changed` last asked, the revision of the latest change, and the revision a background write is
/// saving now.
#[derive(Debug, Default)]
pub struct Pending {
    span: Option<(Instant, Instant)>,
    changed: bool,
    rev: u64,
    flying: Option<u64>,
}

impl Pending {
    pub fn mark(&mut self, now: Instant) {
        self.changed = true;
        self.rev += 1;
        self.span = Some((self.span.map_or(now, |(first, _)| first), now));
    }
    /// Idle while a write is out: its completion wakes the host, which then decides again.
    pub fn decision(&self, now: Instant) -> Flush {
        if self.flying.is_some() {
            return Flush::Idle;
        }
        self.span.map_or(Flush::Idle, |(first, last)| {
            flush_decision_capped(first, last, now)
        })
    }
    /// Write now? Quit writes anything unwritten at once; otherwise only a due flush does.
    pub fn should_write(&self, now: Instant, quit: bool) -> bool {
        (quit && self.span.is_some()) || self.decision(now) == Flush::Now
    }
    /// Everything is on disk (a quit write, or no place to save).
    pub fn written(&mut self) {
        self.span = None;
        self.flying = None;
    }
    pub fn dirty(&self) -> bool {
        self.span.is_some()
    }
    /// The revision the next write saves.
    pub fn rev(&self) -> u64 {
        self.rev
    }
    /// A background write of the current revision started.
    pub fn fly(&mut self) {
        self.flying = Some(self.rev);
    }
    /// A background write of `rev` finished. Only the revision that landed is clean: a change made while it ran
    /// stays pending, and a failed write is retried later.
    pub fn landed(&mut self, rev: u64, ok: bool, now: Instant) {
        self.flying = None;
        if !ok {
            self.retry(now);
        } else if rev == self.rev {
            self.span = None;
        } else if let Some((_, last)) = self.span {
            self.span = Some((last, last));
        }
    }
    pub fn flying(&self) -> bool {
        self.flying.is_some()
    }
    /// True once per change: the host rebuilds the sign-out copy (`keep_unsaved`) only then, not on every pass of a burst.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
    /// A failed write stays pending and is next due `RETRY` after `now`.
    pub fn retry(&mut self, now: Instant) {
        let at = now + RETRY - COALESCE;
        self.span = Some((at, at));
    }
}

/// Reads the file once and closes it. Never fails: any problem yields defaults.
pub fn load(path: &Path) -> (Store, Load) {
    use std::io::Read;
    // At most `MAX_BYTES` + 1 are read: a bigger file is never held, but its head still names its version.
    let mut bytes = Vec::new();
    match std::fs::File::open(path)
        .and_then(|file| file.take(MAX_BYTES + 1).read_to_end(&mut bytes))
    {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (Store::default(), Load::Missing),
        Err(_) => return (Store::default(), Load::Unreadable),
    }
    let oversized = bytes.len() as u64 > MAX_BYTES;
    // Oversized text is only for the version check; it is never parsed.
    let text = if oversized {
        String::from_utf8_lossy(&bytes[..64.min(bytes.len())]).into_owned()
    } else {
        String::from_utf8(bytes).unwrap_or_default()
    };
    match (!oversized).then(|| parse(&text)).flatten() {
        Some(store) => {
            // The Copied list is memory only now: copies an older build saved leave the file at once.
            if text.lines().any(|line| line.starts_with("clip=")) {
                let _ = store.save(path);
            }
            (store, Load::Loaded)
        }
        // A store from a newer build, of any size, is left alone, so going back to that build finds it whole.
        None if version(&text).is_some_and(|v| v > VERSION) => (Store::default(), Load::Unreadable),
        None => {
            // Keep the first bad copy for the user and set this one aside under the next free name (`.bad`,
            // `.1.bad`, ...), never replacing an earlier copy; when no name is free the file is left alone, as if
            // unreadable.
            let free = (0..9)
                .map(|n| match n {
                    0 => path.with_extension("roqa.bad"),
                    n => path.with_extension(format!("roqa.{n}.bad")),
                })
                .find(|bad| !bad.exists());
            #[expect(
                clippy::disallowed_methods,
                reason = "quarantine of a bad store file, never new data"
            )]
            let kept = free.is_some_and(|bad| std::fs::rename(path, bad).is_ok());
            let load = if kept {
                Load::Quarantined
            } else {
                Load::Unreadable
            };
            (Store::default(), load)
        }
    }
}

/// The store to start with and where to save it. A file that exists but cannot be read (locked for a moment by a
/// scanner or a backup) gives defaults and no save path, so this run never writes them over the real file.
pub fn open(path: Option<PathBuf>) -> (Store, Option<PathBuf>) {
    let Some(path) = path else {
        return (Store::default(), None);
    };
    let (store, load) = load(&path);
    if load == Load::Unreadable {
        eprintln!("Store: could not be read; changes are not saved in this run.");
        return (store, None);
    }
    (store, Some(path))
}

fn version(text: &str) -> Option<u32> {
    text.lines()
        .next()?
        .strip_prefix("version=")?
        .trim()
        .parse()
        .ok()
}

fn parse(text: &str) -> Option<Store> {
    let version = version(text)?;
    if version == 0 || version > VERSION {
        return None;
    }
    let mut lines = text.lines();
    lines.next();
    let mut store = Store::default();
    let mut ended = false;
    for line in lines {
        if line == "end" {
            ended = true;
            break;
        }
        match line.split_once('=') {
            Some(("pin", value)) if !value.is_empty() => store.pins.push(value.into()),
            Some(("folder", value)) if !value.is_empty() => store.folders.push(value.into()),
            Some(("hide", value)) if !value.is_empty() => store.hidden.push(value.into()),
            Some(("shelf", value)) if !value.is_empty() => store.shelf.push(value.into()),
            Some(("timer", value)) => store.timers.extend(value.parse::<u64>()),
            Some(("remind", value)) => {
                if let Some((expiry, label)) = value.split_once(',')
                    && let Ok(expiry) = expiry.parse()
                {
                    store
                        .reminders
                        .push((expiry, crate::clipboard::decode(label)));
                }
            }
            Some(("lasttab", value)) => {
                store.last_tab = Tab::from_key(value)
                    .filter(|tab| tab.available())
                    .unwrap_or_default();
            }
            Some(("wdrank", value)) => {
                if let Some((day, n)) = value.split_once(',')
                    && let (Ok(day), Ok(n)) = (day.parse(), n.parse())
                {
                    store.water = (day, n);
                }
            }
            Some(("wxplace", value)) => store.place = Place::from_line(value),
            Some(("wxdata", value)) => store.report = Report::from_line(value),
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
        for (key, list) in [
            ("pin", &self.pins),
            ("folder", &self.folders),
            ("hide", &self.hidden),
            ("shelf", &self.shelf),
        ] {
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
        for (expiry, label) in &self.reminders {
            out.push_str(&format!(
                "remind={expiry},{}\n",
                crate::clipboard::encode(label)
            ));
        }
        out.push_str(&format!("lasttab={}\n", self.last_tab.key()));
        if self.water.1 > 0 {
            out.push_str(&format!("wdrank={},{}\n", self.water.0, self.water.1));
        }
        // A report goes with its place only: a stray one would be shown under the wrong name.
        if let Some(place) = &self.place {
            out.push_str(&format!("wxplace={}\n", place.to_line()));
            if let Some(report) = &self.report {
                out.push_str(&format!("wxdata={}\n", report.to_line()));
            }
        }
        self.settings.write_lines(&mut out);
        out.push_str("end\n");
        out
    }

    /// Atomic temp-and-rename write, same pattern as the opt-in diagnostics. A stale temp file from a crashed
    /// process (a different pid) is ignored by `load` and left in place: at most one tiny file per crash.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        write_text(path, &self.serialize())
    }
}

/// The atomic write of `save`; the file is flushed to disk before the rename, so a power cut cannot leave an
/// empty store in place of the old one.
fn write_text(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;
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
    #[expect(clippy::disallowed_methods, reason = "this is the store's atomic save")]
    std::fs::File::create(&temporary)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temporary, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })
}

/// The unwritten store as text and its revision: what a debounced write saves (on a worker, so the host never waits
/// on the disk) and what a sign-out writes before the debounce, since the window procedure cannot reach the host's
/// store. The host keeps it current while changes wait.
static UNSAVED: std::sync::Mutex<Option<(PathBuf, String, u64)>> = std::sync::Mutex::new(None);
/// The newest revision on disk. Every write holds this lock, so writes never overlap and an older snapshot that
/// arrives late never replaces a newer one.
static NEWEST: std::sync::Mutex<u64> = std::sync::Mutex::new(0);
/// The finished background write: its revision and whether it worked. Read once by `take_done`.
static DONE: std::sync::Mutex<Option<(u64, bool)>> = std::sync::Mutex::new(None);
/// Posted to the host when a background write finished; it only wakes the loop, `take_done` carries the answer.
const SAVED: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 66;

/// Keeps `store` for the next write while `dirty`, and forgets it once written.
pub fn keep_unsaved(path: &Path, store: &Store, dirty: bool, rev: u64) {
    if let Ok(mut unsaved) = UNSAVED.lock() {
        *unsaved = dirty.then(|| (path.to_owned(), store.serialize(), rev));
    }
}

/// Writes `text` unless a revision at least as new already landed; `newest` is held for the whole write.
fn write_rev(newest: &std::sync::Mutex<u64>, path: &Path, text: &str, rev: u64) -> io::Result<()> {
    let mut newest = newest.lock().unwrap_or_else(PoisonError::into_inner);
    if rev <= *newest {
        return Ok(());
    }
    write_text(path, text)?;
    *newest = rev;
    crate::diagnostics::note_store_write();
    Ok(())
}

/// Writes what `keep_unsaved` holds, at once and durably, on this thread: a sign-out (`WM_ENDSESSION`) or quit.
/// It waits for a background write still out, then skips what that already saved. A failed write keeps the copy
/// (unless a newer one arrived meanwhile), so nothing is lost while ROQA still runs.
pub fn save_unsaved() -> io::Result<()> {
    let held = UNSAVED.lock().ok().and_then(|mut unsaved| unsaved.take());
    let Some((path, text, rev)) = held else {
        return Ok(());
    };
    write_rev(&NEWEST, &path, &text, rev).inspect_err(|error| {
        eprintln!("Store: write failed: {error}");
        if let Ok(mut unsaved) = UNSAVED.lock() {
            unsaved.get_or_insert((path, text, rev));
        }
    })
}

/// Starts the debounced write of what `keep_unsaved` holds on one short-lived worker; false when nothing is held
/// or no worker could start. The held copy stays until the host sees the write land.
pub fn save_in_background() -> bool {
    let Some((path, text, rev)) = UNSAVED.lock().ok().and_then(|unsaved| unsaved.clone()) else {
        return false;
    };
    // SAFETY: a scalar thread id query.
    let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    std::thread::Builder::new()
        .name("roqa-store".into())
        .spawn(move || {
            let result = write_rev(&NEWEST, &path, &text, rev);
            if let Err(error) = &result {
                eprintln!("Store: write failed: {error}");
            }
            *DONE.lock().unwrap_or_else(PoisonError::into_inner) = Some((rev, result.is_ok()));
            // A failed post is picked up on the host's next pass.
            let _ = crate::native_window::post_wake(host, SAVED);
        })
        .is_ok()
}

/// The finished background write's revision and success, once.
pub fn take_done() -> Option<(u64, bool)> {
    DONE.lock().unwrap_or_else(PoisonError::into_inner).take()
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "a test writes a temp file, never the store"
)]
mod tests {
    use super::*;
    use crate::weather::Hour;

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
            folders: vec!["Together\tapp:One\tapp:Two".into()],
            pins: vec![
                "Microsoft.Edge_8wekyb3d8bbwe!App".into(),
                r"C:\Apps\a=b.exe".into(),
            ],
            hidden: vec!["app:Hidden.One".into(), r"exe:c:\apps\h.exe".into()],
            shelf: vec![r"C:\Users\me\doc.txt".into()],
            timers: vec![1_800_000_000, 1_800_000_060],
            reminders: vec![(1_800_000_300, "call mum, 17:30".into())],
            settings: Settings {
                dock: false,
                off_monitors: vec![r"\\.\DISPLAY2".into()],
                timer_sound: false,
                shelf_cap: 6,
                diagnostics: Some(r"C:\logs\run=1.json".into()),
                // A tab switched off and the mascot off survive a restart.
                tabs: [true, true, true, false, true, true, true],
                eyes: false,
                water: true,
                water_cfg: [6, 300, 8, 20, 45],
                pomo_cfg: [50, 10, 30, 3],
                ..Settings::default()
            },
            last_tab: Tab::Shelf,
            place: Some(Place {
                lat: 300_444,
                lon: -12_005,
                detected: false,
                name: "Cairo, Egypt".into(),
            }),
            report: Some(Report {
                at: 1_800_000_000,
                tenths: 243,
                code: 2,
                hi: 291,
                lo: 180,
                hours: vec![Hour {
                    hour: 15,
                    tenths: 251,
                    code: 1,
                }],
            }),
            water: (20_400, 3),
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
    fn a_report_is_kept_only_with_its_place_and_junk_weather_lines_are_ignored() {
        let mut orphan = sample();
        orphan.place = None;
        let text = orphan.serialize();
        assert!(!text.contains("wxdata") && !text.contains("wxplace"));
        let parsed = parse("version=2\nwxplace=1,2\nwxdata=x\nend\n").unwrap();
        assert!(parsed.place.is_none() && parsed.report.is_none());
        let lines = sample().serialize();
        assert!(lines.contains("wxplace=300444,-12005,0,Cairo, Egypt\n"));
        assert!(lines.contains("wxdata=1800000000,243,2,291,180,15:251:1\n"));
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
                folders: Vec::new(),
                hidden: Vec::new(),
                pins: vec!["a".into()],
                shelf: vec!["b".into()],
                timers: vec![77], // unparsable timer values are skipped
                reminders: Vec::new(),
                settings: Settings::default(),
                last_tab: Tab::Media,
                place: None,
                report: None,
                water: (0, 0), // junk progress lines are skipped
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
        // The newer file stays where it is and this run does not save over it.
        assert_eq!(load(&dir.file()), (Store::default(), Load::Unreadable));
        assert!(
            std::fs::read_to_string(dir.file())
                .unwrap()
                .contains("pin=keep")
        );
        assert!(!dir.0.join("store.roqa.bad").exists());
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
            // The next save works normally.
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
    fn a_newer_store_is_left_alone_even_when_over_the_cap() {
        let dir = Dir::new("bignewer");
        let body = format!(
            "version={}
pin={}
end
",
            VERSION + 1,
            "x".repeat(MAX_BYTES as usize)
        );
        std::fs::write(dir.file(), &body).unwrap();
        assert_eq!(open(Some(dir.file())), (Store::default(), None));
        assert_eq!(std::fs::read_to_string(dir.file()).unwrap(), body);
        assert!(!dir.0.join("store.roqa.bad").exists());
    }

    #[test]
    fn a_change_is_reported_once_for_the_unsaved_copy() {
        let (mut pending, now) = (Pending::default(), Instant::now());
        assert!(!pending.take_changed());
        pending.mark(now);
        assert!(pending.dirty() && pending.take_changed());
        assert!(
            pending.dirty() && !pending.take_changed(),
            "still unwritten, nothing new"
        );
        pending.retry(now);
        assert!(
            !pending.take_changed(),
            "a failed write is not a new change"
        );
        pending.mark(now);
        pending.written();
        pending.mark(now);
        assert!(pending.take_changed());
    }

    #[test]
    fn a_store_that_cannot_be_set_aside_is_never_saved_over() {
        let dir = Dir::new("stuck");
        std::fs::write(dir.file(), "garbage").unwrap();
        // A folder on every name the bad copy could take leaves no place for it.
        std::fs::create_dir(dir.0.join("store.roqa.bad")).unwrap();
        for n in 1..9 {
            std::fs::create_dir(dir.0.join(format!("store.roqa.{n}.bad"))).unwrap();
        }
        assert_eq!(open(Some(dir.file())), (Store::default(), None));
        assert_eq!(std::fs::read_to_string(dir.file()).unwrap(), "garbage");
    }

    #[test]
    fn an_earlier_bad_copy_is_never_replaced_and_the_next_one_still_moves_aside() {
        let dir = Dir::new("keepfirst");
        let bad = dir.0.join("store.roqa.bad");
        std::fs::write(
            &bad,
            "version=1
pin=original
end
",
        )
        .unwrap();
        for (n, text) in ["garbage", "more garbage"].into_iter().enumerate() {
            std::fs::write(dir.file(), text).unwrap();
            assert_eq!(open(Some(dir.file())), (Store::default(), Some(dir.file())));
            assert!(!dir.file().exists());
            assert!(std::fs::read_to_string(&bad).unwrap().contains("original"));
            let kept = dir.0.join(format!("store.roqa.{}.bad", n + 1));
            assert_eq!(std::fs::read_to_string(kept).unwrap(), text);
        }
    }

    #[test]
    fn an_unreadable_store_is_never_saved_over() {
        let dir = Dir::new("unreadable");
        // A folder where the file should be cannot be read: defaults, and nowhere to save them.
        assert_eq!(load(&dir.0).1, Load::Unreadable);
        assert_eq!(open(Some(dir.0.clone())), (Store::default(), None));
        // A missing file starts fresh and saves there; a readable one loads and saves back.
        assert_eq!(open(Some(dir.file())), (Store::default(), Some(dir.file())));
        sample().save(&dir.file()).unwrap();
        assert_eq!(open(Some(dir.file())), (sample(), Some(dir.file())));
        assert_eq!(open(None), (Store::default(), None));
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
    fn copies_an_older_build_saved_leave_the_file_when_it_loads() {
        let dir = Dir::new("clips");
        let path = dir.file();
        std::fs::write(
            &path,
            "version=2
pin=a
clip=secret token
end
",
        )
        .unwrap();
        assert_eq!(load(&path).0.pins, ["a"]);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("secret"));
    }

    #[test]
    fn save_creates_the_directory() {
        let dir = Dir::new("mkdir");
        let nested = dir.0.join("ROQA").join("store.roqa");
        sample().save(&nested).unwrap();
        assert_eq!(load(&nested).0, sample());
    }

    #[test]
    fn a_sign_out_writes_what_the_host_kept_and_a_failed_write_keeps_it() {
        let dir = Dir::new("signout");
        let path = dir.file();
        keep_unsaved(&path, &sample(), true, 1);
        save_unsaved().unwrap();
        assert_eq!(load(&path).0, sample());
        let _ = std::fs::remove_file(&path);
        keep_unsaved(&path, &sample(), false, 2);
        save_unsaved().unwrap();
        assert!(!path.exists(), "clean: nothing to write");
        // A failed final write (a folder where the file goes) keeps the copy for the next try.
        std::fs::create_dir(&path).unwrap();
        keep_unsaved(&path, &sample(), true, 3);
        assert!(save_unsaved().is_err());
        std::fs::remove_dir(&path).unwrap();
        save_unsaved().unwrap();
        assert_eq!(load(&path).0, sample());
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

    #[test]
    fn a_change_made_during_a_background_write_stays_pending_and_an_older_snapshot_never_wins() {
        let t0 = Instant::now();
        let mut pending = Pending::default();
        pending.mark(t0);
        let first = pending.rev();
        pending.fly();
        assert_eq!(
            pending.decision(t0 + MAX_WAIT * 9),
            Flush::Idle,
            "one write at a time"
        );
        pending.mark(t0 + Duration::from_secs(1));
        pending.landed(first, true, t0 + Duration::from_secs(2));
        assert!(
            pending.dirty() && !pending.flying(),
            "the newer change is still unwritten"
        );
        assert!(pending.should_write(t0 + Duration::from_secs(5), false));
        let second = pending.rev();
        pending.fly();
        pending.landed(second, true, t0 + Duration::from_secs(6));
        assert!(!pending.dirty());
        pending.mark(t0 + Duration::from_secs(7));
        pending.fly();
        pending.landed(pending.rev(), false, t0 + Duration::from_secs(8));
        assert!(pending.dirty(), "a failed write stays pending");

        // The disk side: the newer revision lands first, the older arrives late and is skipped.
        let dir = Dir::new("revision");
        let gate = std::sync::Mutex::new(0);
        write_rev(
            &gate,
            &dir.file(),
            "version=2
newer
",
            2,
        )
        .unwrap();
        write_rev(
            &gate,
            &dir.file(),
            "version=2
older
",
            1,
        )
        .unwrap();
        assert!(
            std::fs::read_to_string(dir.file())
                .unwrap()
                .contains("newer")
        );
        // A failed write does not advance the revision, so the same one can be retried.
        let blocked = Dir::new("revision-blocked");
        std::fs::create_dir(blocked.file()).unwrap();
        assert!(write_rev(&gate, &blocked.file(), "x", 3).is_err());
        assert_eq!(*gate.lock().unwrap(), 2);
    }
}
