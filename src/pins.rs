//! Pinned apps (M5 S3): a pure, owned, ordered list of identities. No I/O, no thread, no taskbar pin import.
//!
//! A pin is just an `Identity` (AUMID, or lowercased exe path as the fallback). Its display name and launch
//! target are derived from it, so the store keeps one string per pin: `app:<aumid>` or `exe:<path>`.

use crate::store::Store;
use crate::window_model::Identity;

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
pub struct Pins(Vec<Identity>);

/// Canonical pin key: exe paths are lowercased (the catalog already does), unusable identities are rejected.
fn normalize(identity: &Identity) -> Option<Identity> {
    let valid = |text: &str| !text.is_empty() && !text.contains(['\n', '\r']);
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
        ("exe", path) => Identity::Executable(path.into()),
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
        pins
    }

    /// Replaces the store's pin section; the caller marks the store dirty.
    pub fn write_to(&self, store: &mut Store) {
        store.pins = self.0.iter().filter_map(encode).collect();
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

    pub fn remove(&mut self, identity: &Identity) -> bool {
        self.position(identity)
            .map(|at| self.0.remove(at))
            .is_some()
    }

    /// Moves one pin to a new index. False (and no change) when either index is out of bounds or equal,
    /// so the caller sets the store dirty flag exactly once per real reorder.
    pub fn move_to(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.0.len() || to >= self.0.len() {
            return false;
        }
        let pin = self.0.remove(from);
        self.0.insert(to, pin);
        true
    }
}

/// Short name for a pin that has no window to take a title from: AUMID package name, or exe file stem.
pub fn display_name(identity: &Identity) -> String {
    let name = match identity {
        Identity::Application(id) => id.split(['!', '_']).next().unwrap_or(id),
        Identity::Executable(path) => {
            let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
            file.strip_suffix(".exe").unwrap_or(file)
        }
        Identity::Unavailable(_) => "",
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
        assert_eq!(
            display_name(&app("Microsoft.Edge_8wekyb3d8bbwe!App")),
            "Microsoft.Edge"
        );
        assert_eq!(display_name(&app("Contoso.Mail")), "Contoso.Mail");
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
