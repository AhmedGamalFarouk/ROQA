//! Pinned apps (M5 S3): an owned, ordered list of identities. No thread or taskbar pin import.
//!
//! A pin is just an `Identity` (AUMID, or lowercased exe path as the fallback). Its display name and launch
//! target are derived from it, so the store keeps one string per pin: `app:<aumid>` or `exe:<path>`.

use crate::store::Store;
use crate::window_model::{Identity, stable_executable};

/// Hard cap so a runaway add loop cannot grow the list or the store (UNVERIFIED value; a few dozen is plenty).
pub const MAX_PINS: usize = 32;

#[derive(Debug, PartialEq, Eq)]
pub enum PinError {
    /// `Identity::Unavailable`, or an empty or multi-line key.
    Unpinnable,
    Duplicate,
    Full,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
/// Pins, folders, and the apps hidden from the dock.
pub struct Pins(Vec<Identity>, Vec<Folder>, Vec<Identity>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub name: String,
    pub members: Vec<Identity>,
}

/// Canonical pin key: exe paths are lowercased (the catalog already does), unusable identities are rejected.
fn normalize(identity: &Identity) -> Option<Identity> {
    let valid = |text: &str| !text.is_empty() && !text.chars().any(char::is_control);
    match identity {
        Identity::Application(id) if valid(id) => Some(identity.clone()),
        Identity::Executable(path) if valid(path) => {
            Some(Identity::Executable(path.to_lowercase()))
        }
        _ => None,
    }
}

fn encode(identity: &Identity) -> Option<String> {
    match identity {
        Identity::Application(id) => Some(format!("app:{id}")),
        Identity::Executable(path) => Some(format!("exe:{path}")),
        Identity::Unavailable(_) => None,
    }
}

fn decode(text: &str) -> Option<Identity> {
    let identity = match text.split_once(':')? {
        ("app", id) => Identity::Application(id.into()),
        // Migrates pins saved before versioned Squirrel paths were keyed on their stub.
        ("exe", path) => Identity::Executable(stable_executable(path.into())),
        _ => return None,
    };
    normalize(&identity)
}

impl Pins {
    /// Loads the store's pin strings; unknown, duplicate and over-cap entries are dropped, order is kept.
    pub fn from_store(store: &Store) -> Self {
        let mut pins = Pins::default();
        for identity in store.pins.iter().filter_map(|text| decode(text)) {
            let _ = pins.add(identity);
        }
        for line in store.folders.iter().take(MAX_PINS / 2) {
            let mut parts = line.split('\t');
            let name = parts.next().unwrap_or_default();
            let mut members = Vec::new();
            for identity in parts.filter_map(decode) {
                if pins.position(&identity).is_some()
                    && pins.folder(&identity).is_none()
                    && !members.contains(&identity)
                {
                    members.push(identity);
                }
            }
            if members.len() >= 2 {
                pins.1.push(Folder {
                    name: folder_name(name),
                    members,
                });
            }
        }
        for identity in store.hidden.iter().filter_map(|text| decode(text)) {
            pins.hide(identity);
        }
        pins
    }

    /// Replaces the store's pin section; the caller marks the store dirty.
    pub fn write_to(&self, store: &mut Store) {
        store.pins = self.0.iter().filter_map(encode).collect();
        store.folders = self
            .1
            .iter()
            .map(|folder| {
                let mut line = folder.name.clone();
                for member in &folder.members {
                    if let Some(key) = encode(member) {
                        line.push('\t');
                        line.push_str(&key);
                    }
                }
                line
            })
            .collect();
        store.hidden = self.2.iter().filter_map(encode).collect();
    }

    pub fn items(&self) -> &[Identity] {
        &self.0
    }

    pub fn position(&self, identity: &Identity) -> Option<usize> {
        let key = normalize(identity)?;
        self.0.iter().position(|pin| *pin == key)
    }

    /// Appends and returns the new index.
    pub fn add(&mut self, identity: Identity) -> Result<usize, PinError> {
        let key = normalize(&identity).ok_or(PinError::Unpinnable)?;
        if self.0.contains(&key) {
            return Err(PinError::Duplicate);
        }
        if self.0.len() >= MAX_PINS {
            return Err(PinError::Full);
        }
        self.0.push(key);
        Ok(self.0.len() - 1)
    }

    pub fn hidden(&self) -> &[Identity] {
        &self.2
    }

    /// Hides an app from the dock; false when it cannot be (no usable key, already hidden, list full).
    pub fn hide(&mut self, identity: Identity) -> bool {
        match normalize(&identity) {
            Some(key) if !self.2.contains(&key) && self.2.len() < MAX_PINS => {
                self.2.push(key);
                true
            }
            _ => false,
        }
    }

    pub fn unhide(&mut self, index: usize) -> bool {
        index < self.2.len() && {
            self.2.remove(index);
            true
        }
    }

    /// Whether a running group is hidden: by its identity, or by its exe when an exe was hidden.
    pub fn hides(&self, identity: &Identity, executable: Option<&str>) -> bool {
        self.2.iter().any(|hidden| {
            hidden == identity
                || matches!(hidden, Identity::Executable(path) if executable == Some(path))
        })
    }

    pub fn remove(&mut self, identity: &Identity) -> bool {
        self.ungroup(identity);
        self.position(identity)
            .map(|at| self.0.remove(at))
            .is_some()
    }

    pub fn folder(&self, identity: &Identity) -> Option<&Folder> {
        self.1
            .iter()
            .find(|folder| folder.members.contains(identity))
    }
    pub fn has_folders(&self) -> bool {
        !self.1.is_empty()
    }

    pub fn group(&mut self, source: &Identity, target: &Identity) -> bool {
        let (Some(source), Some(target)) = (normalize(source), normalize(target)) else {
            return false;
        };
        if source == target
            || self.0.len()
                + usize::from(self.position(&source).is_none())
                + usize::from(self.position(&target).is_none())
                > MAX_PINS
            || self
                .folder(&source)
                .is_some_and(|folder| folder.members.contains(&target))
        {
            return false;
        }
        let _ = self.add(source.clone());
        let _ = self.add(target.clone());
        self.ungroup(&source);
        if let Some(folder) = self
            .1
            .iter_mut()
            .find(|folder| folder.members.contains(&target))
        {
            folder.members.push(source.clone());
        } else {
            self.1.push(Folder {
                name: "Together".into(),
                members: vec![target, source.clone()],
            });
        }
        true
    }

    pub fn ungroup(&mut self, identity: &Identity) -> bool {
        let Some(identity) = normalize(identity) else {
            return false;
        };
        let mut changed = false;
        for folder in &mut self.1 {
            let before = folder.members.len();
            folder.members.retain(|member| member != &identity);
            changed |= before != folder.members.len();
        }
        self.1.retain(|folder| folder.members.len() >= 2);
        changed
    }

    pub fn rename(&mut self, identity: &Identity, name: &str) -> bool {
        let Some(folder) = self
            .1
            .iter_mut()
            .find(|folder| folder.members.contains(identity))
        else {
            return false;
        };
        let name = folder_name(name);
        if folder.name == name {
            return false;
        }
        folder.name = name;
        true
    }

    /// Moves one pin to a new index. False (and no change) when either index is out of bounds or equal,
    /// so the caller sets the store dirty flag exactly once per real reorder.
    pub fn move_to(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.0.len() || to >= self.0.len() {
            return false;
        }
        let source = self.0[from].clone();
        let target = self.0[to].clone();
        let members = self
            .folder(&source)
            .map_or_else(|| vec![source], |folder| folder.members.clone());
        if members.contains(&target) {
            return false;
        }
        let targets = self
            .folder(&target)
            .map_or_else(|| vec![target], |folder| folder.members.clone());
        self.0.retain(|pin| !members.contains(pin));
        let positions = self
            .0
            .iter()
            .enumerate()
            .filter(|(_, pin)| targets.contains(pin))
            .map(|(index, _)| index);
        let at = if from < to {
            positions.max().unwrap_or(0) + 1
        } else {
            positions.min().unwrap_or(0)
        };
        self.0.splice(at..at, members);
        true
    }
}

fn folder_name(name: &str) -> String {
    let name: String = name
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(40)
        .collect();
    if name.is_empty() {
        "Together".into()
    } else {
        name
    }
}

/// Shared presentation name for dock labels and folder rows. Missing shell metadata never exposes an AUMID.
pub fn display_name(identity: &Identity) -> String {
    if let Identity::Application(id) = identity {
        use windows::{
            Win32::{
                System::Com::CoTaskMemFree,
                UI::Shell::{IShellItem, SHCreateItemFromParsingName, SIGDN_NORMALDISPLAY},
            },
            core::HSTRING,
        };
        let target = HSTRING::from(format!(r"shell:AppsFolder\{id}"));
        // SAFETY: Shell metadata is queried only on demand; its allocated string is always freed.
        let name = unsafe {
            SHCreateItemFromParsingName::<_, _, IShellItem>(&target, None)
                .and_then(|item| item.GetDisplayName(SIGDN_NORMALDISPLAY))
                .ok()
                .map(|value| {
                    let name = value.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(value.0.cast()));
                    name
                })
        };
        return name
            .filter(|name| {
                !name.is_empty()
                    && !name.contains(id)
                    && name != id.split(['!', '_']).next().unwrap_or(id)
                    && !name.contains('!')
            })
            .unwrap_or_else(|| "App".into());
    }
    let name = match identity {
        Identity::Executable(path) => {
            let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
            file.strip_suffix(".exe").unwrap_or(file)
        }
        _ => "",
    };
    if name.is_empty() { "App" } else { name }.to_string()
}

/// Shell target for launching a not-running pin; validated only when the user activates it, never scanned.
pub fn launch_target(identity: &Identity) -> Option<String> {
    match normalize(identity)? {
        Identity::Application(id) => Some(format!(r"shell:AppsFolder\{id}")),
        // A relative path would resolve against the search path or the working directory; only absolute ones launch.
        Identity::Executable(path) => std::path::Path::new(&path).is_absolute().then_some(path),
        Identity::Unavailable(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_round_trip_and_dissolve_without_losing_pins() {
        let mut pins = Pins::default();
        assert!(pins.group(&app("A"), &app("B")));
        assert!(pins.group(&app("C"), &app("B")));
        assert!(pins.rename(&app("B"), "  Daily\tapps\n  "));
        let mut store = Store::default();
        pins.write_to(&mut store);
        assert_eq!(Pins::from_store(&store), pins);
        assert_eq!(pins.folder(&app("A")).unwrap().name, "Dailyapps");
        assert!(!pins.group(&app("A"), &app("B")));
        assert!(pins.ungroup(&app("A")));
        assert_eq!(pins.folder(&app("B")).unwrap().members.len(), 2);
        assert!(pins.remove(&app("C")));
        assert!(!pins.has_folders());
        assert_eq!(pins.items().len(), 2);
    }

    #[test]
    fn moving_a_folder_keeps_its_members_together() {
        let mut pins = Pins::default();
        for name in ["A", "B", "C", "D"] {
            pins.add(app(name)).unwrap();
        }
        assert!(pins.group(&app("A"), &app("C")));
        assert!(pins.move_to(0, 3));
        assert_eq!(pins.items(), &[app("B"), app("D"), app("C"), app("A")]);
        assert!(!pins.move_to(2, 3));
    }

    #[test]
    fn corrupt_folder_metadata_cannot_duplicate_or_hide_pins() {
        let store = Store {
            pins: vec!["app:A".into(), "app:B".into(), "app:C".into()],
            folders: vec![
                "\tapp:A\tapp:A\tapp:B\tapp:Missing".into(),
                "Other\tapp:B\tapp:C".into(),
            ],
            ..Store::default()
        };
        let pins = Pins::from_store(&store);
        assert_eq!(
            pins.folder(&app("A")).unwrap().members,
            [app("A"), app("B")]
        );
        assert_eq!(pins.folder(&app("A")).unwrap().name, "Together");
        assert!(pins.folder(&app("C")).is_none());
        let mut full = Pins::default();
        for index in 0..MAX_PINS {
            full.add(app(&format!("App{index}"))).unwrap();
        }
        let before = full.clone();
        assert!(!full.group(&app("New"), &app("App0")));
        assert_eq!(full, before);
    }

    fn app(id: &str) -> Identity {
        Identity::Application(id.into())
    }
    fn exe(path: &str) -> Identity {
        Identity::Executable(path.into())
    }

    #[test]
    fn add_dedupes_normalizes_and_caps() {
        let mut pins = Pins::default();
        assert_eq!(pins.add(app("A!App")), Ok(0));
        assert_eq!(pins.add(exe(r"C:\Tools\Edit.EXE")), Ok(1));
        assert_eq!(pins.items()[1], exe(r"c:\tools\edit.exe"));
        assert_eq!(
            pins.add(exe(r"c:\TOOLS\edit.exe")),
            Err(PinError::Duplicate)
        );
        assert_eq!(pins.add(app("A!App")), Err(PinError::Duplicate));
        for bad in [Identity::Unavailable(4), app(""), exe("a\nb")] {
            assert_eq!(pins.add(bad), Err(PinError::Unpinnable));
        }
        for n in pins.items().len()..MAX_PINS {
            assert_eq!(pins.add(app(&format!("P{n}"))), Ok(n));
        }
        assert_eq!(pins.add(app("one-too-many")), Err(PinError::Full));
        assert_eq!(pins.items().len(), MAX_PINS);
    }

    #[test]
    fn remove_and_position_use_the_canonical_key() {
        let mut pins = Pins::default();
        pins.add(exe(r"c:\a.exe")).unwrap();
        pins.add(app("B")).unwrap();
        assert_eq!(pins.position(&exe(r"C:\A.EXE")), Some(0));
        assert!(!pins.remove(&app("missing")));
        assert!(!pins.remove(&Identity::Unavailable(1)));
        assert!(pins.remove(&exe(r"C:\A.exe")));
        assert_eq!(pins.items(), [app("B")]);
    }

    #[test]
    fn reorder_bounds() {
        let mut pins = Pins::default();
        for id in ["a", "b", "c"] {
            pins.add(app(id)).unwrap();
        }
        let before = pins.clone();
        for (from, to) in [(0, 3), (3, 0), (1, 1), (9, 9), (usize::MAX, 0)] {
            assert!(!pins.move_to(from, to), "{from}->{to}");
            assert_eq!(pins, before);
        }
        assert!(pins.move_to(0, 2));
        assert_eq!(pins.items(), [app("b"), app("c"), app("a")]);
        assert!(pins.move_to(2, 0));
        assert_eq!(pins.items(), [app("a"), app("b"), app("c")]);
        assert!(!Pins::default().move_to(0, 0));
    }

    #[test]
    fn names_and_launch_targets() {
        assert_eq!(display_name(&app("ROQA.Missing_123!App")), "App");
        assert_eq!(display_name(&exe(r"c:\tools\edit.exe")), "edit");
        assert_eq!(display_name(&Identity::Unavailable(1)), "App");
        assert_eq!(display_name(&app("!x")), "App");
        assert_eq!(
            launch_target(&app("A!App")).unwrap(),
            r"shell:AppsFolder\A!App"
        );
        assert_eq!(launch_target(&exe(r"C:\X.exe")).unwrap(), r"c:\x.exe");
        assert_eq!(launch_target(&Identity::Unavailable(1)), None);
    }

    #[test]
    fn store_round_trip_through_a_real_file() {
        let mut pins = Pins::default();
        pins.add(app("Microsoft.Edge_8wekyb3d8bbwe!App")).unwrap();
        pins.add(exe(r"c:\apps\a=b.exe")).unwrap();
        let mut store = Store::default();
        pins.write_to(&mut store);
        assert_eq!(
            store.pins,
            [
                "app:Microsoft.Edge_8wekyb3d8bbwe!App",
                r"exe:c:\apps\a=b.exe"
            ]
        );
        let path = std::env::temp_dir().join(format!("roqa-pins-test-{}.roqa", std::process::id()));
        store.save(&path).unwrap();
        let (loaded, state) = crate::store::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(state, crate::store::Load::Loaded);
        assert_eq!(Pins::from_store(&loaded), pins);
    }

    #[test]
    fn hidden_apps_match_by_identity_or_exe_and_survive_the_store() {
        let mut pins = Pins::default();
        assert!(pins.hide(app("A!App")) && pins.hide(exe(r"C:\X.exe")));
        assert!(!pins.hide(app("A!App")) && !pins.hide(Identity::Unavailable(1)));
        assert!(pins.hides(&app("A!App"), None));
        assert!(pins.hides(&app("B!App"), Some(r"c:\x.exe")));
        assert!(!pins.hides(&app("B!App"), None));
        let mut store = Store::default();
        pins.write_to(&mut store);
        assert_eq!(Pins::from_store(&store), pins);
        assert!(pins.unhide(0) && !pins.unhide(5));
        assert_eq!(pins.hidden(), [exe(r"c:\x.exe")]);
    }

    #[test]
    fn from_store_drops_junk_duplicates_and_overflow() {
        let junk = ["app:A", "bogus", "exe:", "app:A", "exe:C:\\B.exe", "app:"];
        let store = Store {
            pins: (junk.iter().map(|s| s.to_string()))
                .chain((0..MAX_PINS + 5).map(|n| format!("app:X{n}")))
                .collect(),
            ..Store::default()
        };
        let pins = Pins::from_store(&store);
        assert_eq!(pins.items().len(), MAX_PINS);
        assert_eq!(&pins.items()[..2], [app("A"), exe(r"c:\b.exe")]);
        assert_eq!(Pins::from_store(&Store::default()), Pins::default());
    }

    #[test]
    fn only_absolute_executable_paths_launch() {
        for bad in ["edit.exe", r"tools\edit.exe", "cmd", r"\x.exe"] {
            assert_eq!(launch_target(&exe(bad)), None, "{bad}");
        }
        assert!(launch_target(&exe(r"\\host\share\a.exe")).is_some());
    }
}
