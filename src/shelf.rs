//! File shelf (M5 S6): a pure, ordered list of file references. An entry is a path and its display name;
//! ROQA never reads, copies or thumbnails the file, and this module does no I/O, so a UNC path is only ever
//! marked "not checked" here. Whether a target still exists is learned at activation (`mark_missing`).

use crate::store::Store;

/// Hard ceiling and default of the entries kept (UNVERIFIED default from m5-preparation.md section 4). A new
/// one evicts the oldest beyond the shelf's cap.
pub const MAX: usize = 12;
/// Entries drawn and exposed to UI Automation: the newest ones fit the panel's free pocket (PROVISIONAL).
pub const VISIBLE: usize = 3;
/// Longest accepted path, in chars; keeps a hostile drop from filling the store.
pub const MAX_PATH_CHARS: usize = 1024;
/// A drop reads at most this many items.
pub const MAX_DROP: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    /// Empty, too long, or holding characters no file name has (or the store line format cannot keep).
    Invalid,
    /// `\\.\`, `\\?\`, `\??\` namespaces and reserved device names such as `NUL`.
    Device,
    /// Not a drive-letter or UNC path.
    Relative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// Existence is unknown until the user activates the entry.
    Unknown,
    /// A UNC path: never touched by the host thread, so never checked.
    NotChecked,
    /// The last activation found nothing there. Kept, and retried on the next activation.
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub check: Check,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shelf {
    entries: Vec<Entry>,
    /// Bumped on every visible change, so UI Automation retires elements of an older list.
    revision: u32,
    /// Entries kept, `1..=MAX` (the settings' shelf size).
    cap: usize,
    page: usize,
}

impl Default for Shelf {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            revision: 0,
            cap: MAX,
            page: 0,
        }
    }
}

const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// `GetDriveType` value of a mapped network drive (`DRIVE_REMOTE`).
pub const DRIVE_REMOTE: u32 = 4;

/// Activation must keep off the host thread for a UNC path and for a drive letter that maps to a share.
pub fn is_network(check: Check, drive_type: u32) -> bool {
    check == Check::NotChecked || drive_type == DRIVE_REMOTE
}

/// Case-insensitive key two spellings of one path share (no file system access, so no canonicalisation).
fn key(path: &str) -> String {
    path.to_lowercase()
}

/// Checks and normalises one path: `/` becomes `\` and a trailing `\` goes (except a root's).
pub fn validate(path: &str) -> Result<Entry, Reject> {
    if path.is_empty() || path.chars().count() > MAX_PATH_CHARS {
        return Err(Reject::Invalid);
    }
    let path = path.replace('/', "\\");
    // Before the character check: `\\?\` itself holds a `?`.
    if ["\\\\.\\", "\\\\?\\", "\\??\\"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        return Err(Reject::Device);
    }
    if path.contains(|c: char| c.is_control() || "*?<>|\"".contains(c)) {
        return Err(Reject::Invalid);
    }
    let unc = path.starts_with("\\\\");
    let bytes = path.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    if !unc && !drive {
        return Err(Reject::Relative);
    }
    // After a drive's `X:`, any other colon names an alternate data stream.
    if path[if drive { 2 } else { 0 }..].contains(':') {
        return Err(Reject::Invalid);
    }
    let root = if drive { 3 } else { 0 };
    let trimmed = path[root..].trim_end_matches('\\');
    let path = format!("{}{trimmed}", &path[..root]);
    let parts: Vec<&str> = path[root..].split('\\').collect();
    if unc && (parts.len() < 4 || parts[2].is_empty() || parts[3].is_empty()) {
        return Err(Reject::Invalid); // needs `\\server\share`
    }
    let last = parts.last().copied().unwrap_or("");
    let stem = last.trim_end_matches([' ', '.']);
    let stem = stem.split('.').next().unwrap_or(stem).to_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(Reject::Device);
    }
    let name = if last.is_empty() { &path } else { last };
    Ok(Entry {
        name: name.to_owned(),
        check: if unc {
            Check::NotChecked
        } else {
            Check::Unknown
        },
        path,
    })
}

impl Shelf {
    /// Loads the store's references; invalid, duplicate and over-cap lines are dropped, order is kept.
    pub fn from_store(store: &Store) -> Self {
        let mut shelf = Shelf::default();
        for entry in store.shelf.iter().filter_map(|path| validate(path).ok()) {
            if shelf.entries.len() < MAX && !shelf.has(&entry.path) {
                shelf.entries.push(entry);
            }
        }
        shelf
    }

    /// Replaces the store's shelf section; true when it changed, so the caller marks the store dirty.
    pub fn write_to(&self, store: &mut Store) -> bool {
        let paths: Vec<String> = self.entries.iter().map(|e| e.path.clone()).collect();
        let changed = store.shelf != paths;
        store.shelf = paths;
        changed
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    /// The entries drawn and exposed to UI Automation.
    pub fn visible(&self) -> &[Entry] {
        let start = self.page_start();
        &self.entries
            [start..(start + if self.overflow() { 2 } else { VISIBLE }).min(self.entries.len())]
    }
    pub fn overflow(&self) -> bool {
        self.entries.len() > VISIBLE
    }
    fn page_start(&self) -> usize {
        if self.overflow() {
            (self.page % self.entries.len().div_ceil(2)) * 2
        } else {
            0
        }
    }
    pub fn absolute(&self, slot: usize) -> Option<usize> {
        (slot < self.visible().len()).then(|| self.page_start() + slot)
    }
    pub fn slot_count(&self) -> usize {
        (self.visible().len() + usize::from(self.overflow())).max(1)
    }
    pub fn slot_label(&self, slot: usize) -> Option<String> {
        if self.is_empty() && slot == 0 {
            return Some("Add shelf files (Ctrl+O)".to_owned());
        }
        if self.overflow() && slot == self.visible().len() {
            Some(format!(
                "More shelf files, page {} of {}",
                self.page_start() / 2 + 1,
                self.entries.len().div_ceil(2)
            ))
        } else {
            self.absolute(slot).and_then(|index| self.label(index))
        }
    }
    pub fn next_page(&mut self) -> bool {
        if !self.overflow() {
            return false;
        }
        self.page = (self.page_start() / 2 + 1) % self.entries.len().div_ceil(2);
        self.revision = self.revision.wrapping_add(1);
        true
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn revision(&self) -> u32 {
        self.revision
    }

    fn has(&self, path: &str) -> bool {
        self.entries.iter().any(|e| key(&e.path) == key(path))
    }

    /// Adds a dropped or chosen list: the valid, new items go to the front in list order (at most `MAX`),
    /// evicting the oldest beyond the cap. Returns how many were added.
    pub fn add_all<S: AsRef<str>>(&mut self, paths: &[S]) -> usize {
        let mut fresh: Vec<Entry> = Vec::new();
        for entry in paths
            .iter()
            .take(MAX_DROP)
            .filter_map(|p| validate(p.as_ref()).ok())
        {
            if fresh.len() < self.cap
                && !self.has(&entry.path)
                && !fresh.iter().any(|f| key(&f.path) == key(&entry.path))
            {
                fresh.push(entry);
            }
        }
        let added = fresh.len();
        if added > 0 {
            fresh.append(&mut self.entries);
            fresh.truncate(self.cap);
            self.entries = fresh;
            self.revision = self.revision.wrapping_add(1);
        }
        added
    }

    /// Sets the cap within `1..=MAX`; true when entries beyond it were dropped (the oldest go first).
    pub fn set_cap(&mut self, cap: usize) -> bool {
        self.cap = cap.clamp(1, MAX);
        let cut = self.entries.len() > self.cap;
        if cut {
            self.entries.truncate(self.cap);
            self.revision = self.revision.wrapping_add(1);
        }
        cut
    }

    /// Removes an entry; false for an index past the end.
    pub fn remove(&mut self, index: usize) -> bool {
        let found = index < self.entries.len();
        if found {
            self.entries.remove(index);
            self.revision = self.revision.wrapping_add(1);
        }
        found
    }

    /// An activation found the target gone (or present again): true when the shown state changed.
    pub fn set_missing(&mut self, index: usize, missing: bool) -> bool {
        let Some(entry) = self.entries.get_mut(index) else {
            return false;
        };
        let check = match (missing, entry.check) {
            (_, Check::NotChecked) => return false,
            (true, _) => Check::Missing,
            (false, _) => Check::Unknown,
        };
        let changed = entry.check != check;
        entry.check = check;
        self.revision = self.revision.wrapping_add(u32::from(changed));
        changed
    }

    /// The accessible name of a visible entry, such as "Shelf: notes.txt, file not found".
    pub fn label(&self, index: usize) -> Option<String> {
        let entry = self.entries.get(index)?;
        let note = match entry.check {
            Check::Unknown => "",
            Check::NotChecked => ", network location, not checked",
            Check::Missing => ", file not found",
        };
        Some(format!("Shelf: {}{note}", entry.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(shelf: &Shelf) -> Vec<&str> {
        shelf.entries().iter().map(|e| e.path.as_str()).collect()
    }

    #[test]
    fn overflow_pages_visit_every_reference_and_wrap() {
        let mut shelf = Shelf::default();
        shelf.add_all(&(0..12).map(|n| format!(r"C:\f{n}")).collect::<Vec<_>>());
        let mut visited = Vec::new();
        for _ in 0..6 {
            for slot in 0..shelf.visible().len() {
                visited.push(shelf.absolute(slot).unwrap());
            }
            assert!(
                shelf
                    .slot_label(shelf.visible().len())
                    .unwrap()
                    .starts_with("More shelf files")
            );
            assert!(shelf.next_page());
        }
        assert_eq!(visited, (0..12).collect::<Vec<_>>());
        assert_eq!(shelf.absolute(0), Some(0));
        shelf.set_cap(1);
        assert_eq!(shelf.absolute(0), Some(0));
        assert!(!shelf.next_page());
    }

    #[test]
    fn validation_accepts_drive_and_unc_paths_and_normalises_them() {
        let entry = validate("C:/Users/me/notes.txt").unwrap();
        assert_eq!(
            (entry.path.as_str(), entry.name.as_str(), entry.check),
            (r"C:\Users\me\notes.txt", "notes.txt", Check::Unknown)
        );
        assert_eq!(validate(r"D:\Photos\").unwrap().path, r"D:\Photos");
        assert_eq!(validate(r"D:\Photos\").unwrap().name, "Photos");
        let root = validate(r"C:\").unwrap();
        assert_eq!((root.path.as_str(), root.name.as_str()), (r"C:\", r"C:\"));
        // A dotted name that merely starts like a device name is a file name.
        assert!(validate(r"C:\data\console.log").is_ok());
        assert!(validate(r"C:\data\a.b.txt").is_ok());
    }

    #[test]
    fn unc_paths_are_marked_not_checked() {
        let entry = validate(r"\\server\share\docs\a.pdf").unwrap();
        assert_eq!(entry.check, Check::NotChecked);
        assert_eq!(entry.name, "a.pdf");
        assert_eq!(validate(r"\\server\share").unwrap().name, "share");
        assert_eq!(validate(r"\\server"), Err(Reject::Invalid));
        assert_eq!(validate(r"\\server\"), Err(Reject::Invalid));
    }

    #[test]
    fn device_relative_and_malformed_paths_are_rejected() {
        for device in [
            r"\\.\PhysicalDrive0",
            r"\\?\GLOBALROOT\Device\x",
            r"\\?\C:\x",
            r"\??\C:\x",
            r"C:\dir\NUL",
            r"C:\dir\nul.txt",
            r"C:\dir\Com1",
            r"C:\dir\con ",
        ] {
            assert_eq!(validate(device), Err(Reject::Device), "{device}");
        }
        for relative in [
            r"notes.txt",
            r"..\x",
            r"\rooted",
            r"C:x",
            r"C:",
            "file:///C:/x",
        ] {
            assert_eq!(validate(relative), Err(Reject::Relative), "{relative}");
        }
        for bad in [
            "",
            "C:\\a\nb",
            r"C:\a*b",
            r"C:\a:stream",
            r"C:\a|b",
            "C:\\a\0b",
        ] {
            assert_eq!(validate(bad), Err(Reject::Invalid), "{bad:?}");
        }
        assert_eq!(
            validate(&format!(r"C:\{}", "x".repeat(MAX_PATH_CHARS))),
            Err(Reject::Invalid)
        );
    }

    #[test]
    fn dedupe_ignores_case_slashes_and_a_trailing_separator() {
        let mut shelf = Shelf::default();
        assert_eq!(shelf.add_all(&[r"C:\A\b.txt"]), 1);
        let revision = shelf.revision();
        assert_eq!(shelf.add_all(&["c:/a/B.TXT", r"C:\A\b.txt\"]), 0);
        assert_eq!((shelf.entries().len(), shelf.revision()), (1, revision));
    }

    #[test]
    fn a_drop_list_adds_valid_new_items_in_list_order_at_the_front() {
        let mut shelf = Shelf::default();
        shelf.add_all(&[r"C:\old.txt"]);
        let added = shelf.add_all(&[
            r"C:\one.txt",
            r"relative.txt",       // rejected
            r"\\.\PhysicalDrive0", // rejected
            r"C:\two.txt",
            r"C:\ONE.txt", // duplicate inside the list
            r"C:\old.txt", // already on the shelf
            r"C:\three.txt",
        ]);
        assert_eq!(added, 3);
        assert_eq!(
            paths(&shelf),
            [r"C:\one.txt", r"C:\two.txt", r"C:\three.txt", r"C:\old.txt"]
        );
        // Nothing valid: nothing changes, so nothing is repainted or written.
        let revision = shelf.revision();
        assert_eq!(shelf.add_all::<&str>(&[]), 0);
        assert_eq!(shelf.add_all(&["x", ""]), 0);
        assert_eq!(shelf.revision(), revision);
    }

    #[test]
    fn the_cap_evicts_the_oldest_and_a_drop_reads_a_bounded_list() {
        let mut shelf = Shelf::default();
        for n in 0..MAX {
            shelf.add_all(&[format!(r"C:\f{n}.txt")]);
        }
        assert_eq!(shelf.entries().len(), MAX);
        assert_eq!(shelf.entries()[0].path, r"C:\f11.txt");
        shelf.add_all(&[r"C:\new.txt"]);
        assert_eq!(shelf.entries().len(), MAX);
        assert_eq!(shelf.entries()[0].path, r"C:\new.txt");
        assert!(!paths(&shelf).contains(&r"C:\f0.txt")); // the oldest went
        // A huge drop keeps the first MAX of the list and ignores items past MAX_DROP.
        let mut shelf = Shelf::default();
        let many: Vec<String> = (0..MAX_DROP + 10)
            .map(|n| format!(r"C:\d{n}.txt"))
            .collect();
        assert_eq!(shelf.add_all(&many), MAX);
        assert_eq!(paths(&shelf)[0], r"C:\d0.txt");
        assert_eq!(shelf.slot_count(), VISIBLE);
    }

    #[test]
    fn remove_and_missing_transitions_bump_the_revision_only_on_a_real_change() {
        let mut shelf = Shelf::default();
        shelf.add_all(&[r"C:\a.txt", r"\\srv\share\b.txt"]);
        let revision = shelf.revision();
        assert!(shelf.set_missing(0, true));
        assert_eq!(shelf.entries()[0].check, Check::Missing);
        assert_eq!(shelf.label(0).unwrap(), "Shelf: a.txt, file not found");
        assert!(!shelf.set_missing(0, true)); // already known
        assert!(shelf.set_missing(0, false)); // present again at the next activation
        assert_eq!(shelf.entries()[0].check, Check::Unknown);
        // A UNC entry stays "not checked" whatever an activation reports.
        assert!(!shelf.set_missing(1, true));
        assert_eq!(shelf.entries()[1].check, Check::NotChecked);
        assert_eq!(
            shelf.label(1).unwrap(),
            "Shelf: b.txt, network location, not checked"
        );
        assert!(!shelf.set_missing(9, true));
        assert!(shelf.revision() > revision);
        let revision = shelf.revision();
        assert!(!shelf.remove(5));
        assert_eq!(shelf.revision(), revision);
        assert!(shelf.remove(0));
        assert_eq!(paths(&shelf), [r"\\srv\share\b.txt"]);
        assert_ne!(shelf.revision(), revision);
    }

    #[test]
    fn the_store_round_trip_keeps_order_and_drops_bad_lines() {
        let mut shelf = Shelf::default();
        shelf.add_all(&[r"C:\a.txt", r"\\srv\share\b.txt", r"C:\c.txt"]);
        let mut store = Store::default();
        assert!(shelf.write_to(&mut store));
        assert!(
            !shelf.write_to(&mut store),
            "an unchanged shelf must not mark the store dirty"
        );
        let dir = std::env::temp_dir().join(format!("roqa-shelf-test-{}", std::process::id()));
        let file = dir.join("store.roqa");
        store.save(&file).unwrap();
        let (loaded, _) = crate::store::load(&file);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Shelf::from_store(&loaded).entries(), shelf.entries());
        // A hand-edited or corrupt store: junk, device paths, duplicates and the cap are filtered.
        let hostile = Store {
            shelf: [r"C:\ok.txt", "junk", r"\\.\C:", r"c:\OK.txt"]
                .into_iter()
                .map(String::from)
                .chain((0..30).map(|n| format!(r"C:\x{n}")))
                .collect(),
            ..Store::default()
        };
        let loaded = Shelf::from_store(&hostile);
        assert_eq!(loaded.entries().len(), MAX);
        assert_eq!(loaded.entries()[0].path, r"C:\ok.txt");
    }

    #[test]
    fn a_lower_cap_drops_the_oldest_and_a_mapped_drive_counts_as_network() {
        let mut shelf = Shelf::default();
        shelf.add_all(&[r"C:\a", r"C:\b", r"C:\c", r"C:\d"]);
        let revision = shelf.revision();
        assert!(!shelf.set_cap(99), "clamped to the ceiling: nothing cut");
        assert!(shelf.set_cap(2));
        assert_eq!(paths(&shelf), [r"C:\a", r"C:\b"]);
        assert_ne!(shelf.revision(), revision);
        assert_eq!(shelf.add_all(&[r"C:\e"]), 1);
        assert_eq!(
            paths(&shelf),
            [r"C:\e", r"C:\a"],
            "new entries obey the cap"
        );
        assert!(shelf.set_cap(0), "0 clamps to 1");
        assert_eq!(shelf.entries().len(), 1);
        // UNC (not checked) or DRIVE_REMOTE stays off the host thread; fixed, removable and unknown do not.
        assert!(is_network(Check::NotChecked, 3));
        assert!(is_network(Check::Unknown, DRIVE_REMOTE));
        assert!(is_network(Check::Missing, DRIVE_REMOTE));
        for drive_type in [0, 1, 2, 3, 5, 6] {
            assert!(!is_network(Check::Unknown, drive_type), "{drive_type}");
        }
    }
}
