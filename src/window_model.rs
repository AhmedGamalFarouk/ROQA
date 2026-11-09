use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Identity {
    /// Explicit AppUserModelID: packaged apps and apps that group themselves on the taskbar.
    Application(String),
    Executable(String),
    Unavailable(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WindowKey {
    pub handle: usize,
    pub process: u32,
}

#[derive(Clone, Debug)]
pub struct WindowRecord {
    pub key: WindowKey,
    pub title: String,
    pub identity: Identity,
    /// Lowercased image path when the process could be queried; icon fallback for AUMID groups.
    pub executable: Option<String>,
    pub minimized: bool,
    pub foreground: bool,
}

#[derive(Clone, Copy, Default)]
pub struct SourceStats {
    pub full_scans: u64,
    pub delta_batches: u64,
    pub inspected: u64,
}
pub struct Snapshot {
    pub generation: u64,
    pub stats: SourceStats,
    pub windows: Vec<WindowRecord>,
    pub complete: bool,
    pub reason: Option<String>,
}

#[derive(Default)]
pub struct WindowModel {
    pub generation: u64,
    pub windows: Vec<WindowRecord>,
    pub incomplete: Option<String>,
}
impl WindowModel {
    pub fn apply(&mut self, snapshot: Snapshot) -> bool {
        if snapshot.generation <= self.generation {
            return false;
        }
        self.generation = snapshot.generation;
        self.incomplete = snapshot.reason;
        if snapshot.complete {
            self.windows = snapshot.windows;
            true
        } else {
            // A truncated scan cannot prove an absent window was destroyed.
            false
        }
    }
    pub fn groups(&self) -> BTreeMap<&Identity, Vec<WindowKey>> {
        let mut groups = BTreeMap::new();
        for window in &self.windows {
            groups
                .entry(&window.identity)
                .or_insert_with(Vec::new)
                .push(window.key);
        }
        groups
    }
}

// One scan in flight; any intervening invalidation requests one newer snapshot.
#[derive(Default)]
pub struct ScanSchedule {
    requested: u64,
    in_flight: Option<u64>,
    dirty: bool,
}
impl ScanSchedule {
    pub fn invalidate(&mut self) {
        self.requested = self.requested.saturating_add(1);
        self.dirty = true;
    }
    pub fn request(&mut self) -> Option<u64> {
        if self.in_flight.is_some() || !self.dirty {
            return None;
        }
        let generation = self.requested;
        self.in_flight = Some(generation);
        self.dirty = false;
        Some(generation)
    }
    pub fn has_pending(&self) -> bool {
        self.dirty && self.in_flight.is_none()
    }
    pub fn complete(&mut self, generation: u64) -> bool {
        if self.in_flight != Some(generation) {
            return false;
        }
        self.in_flight = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(handle: usize, process: u32, title: &str, identity: Identity) -> WindowRecord {
        WindowRecord {
            key: WindowKey { handle, process },
            title: title.into(),
            identity,
            executable: None,
            minimized: false,
            foreground: false,
        }
    }
    fn snapshot(generation: u64, windows: Vec<WindowRecord>) -> Snapshot {
        Snapshot {
            generation,
            stats: SourceStats::default(),
            windows,
            complete: true,
            reason: None,
        }
    }
    #[test]
    fn stale_generation_cannot_resurrect_closed_or_reused_handles() {
        let mut model = WindowModel::default();
        model.apply(snapshot(
            2,
            vec![record(7, 10, "old", Identity::Unavailable(10))],
        ));
        model.apply(snapshot(
            4,
            vec![record(7, 11, "new", Identity::Unavailable(11))],
        ));
        assert!(!model.apply(snapshot(3, vec![])));
        assert_eq!(model.windows[0].key.process, 11);
        assert_eq!(model.windows[0].title, "new");
        assert!(model.apply(snapshot(5, vec![])));
        assert!(model.windows.is_empty());
    }
    #[test]
    fn incomplete_snapshot_retains_last_complete_model_and_marks_uncertainty() {
        let mut model = WindowModel::default();
        model.apply(snapshot(
            1,
            vec![record(7, 10, "kept", Identity::Unavailable(10))],
        ));
        assert!(!model.apply(Snapshot {
            generation: 2,
            stats: SourceStats::default(),
            windows: vec![],
            complete: false,
            reason: Some("limit".into())
        }));
        assert_eq!(model.windows.len(), 1);
        assert_eq!(model.incomplete.as_deref(), Some("limit"));
        model.apply(snapshot(3, vec![]));
        assert!(model.incomplete.is_none());
    }
    #[test]
    fn executable_grouping_never_merges_unavailable_process_identities() {
        let mut model = WindowModel::default();
        model.apply(snapshot(
            1,
            vec![
                record(1, 10, "one", Identity::Executable("app.exe".into())),
                record(2, 20, "two", Identity::Executable("app.exe".into())),
                record(3, 30, "three", Identity::Unavailable(30)),
                record(4, 40, "four", Identity::Unavailable(40)),
            ],
        ));
        let groups = model.groups();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[&Identity::Executable("app.exe".into())].len(), 2);
    }
    #[test]
    fn event_storm_coalesces_without_starving_publication() {
        let mut schedule = ScanSchedule::default();
        schedule.invalidate();
        let first = schedule.request().unwrap();
        for _ in 0..10_000 {
            schedule.invalidate();
            assert_eq!(schedule.request(), None);
        }
        // A complete in-order snapshot is publishable even while a newer refresh is pending.
        // WindowModel separately rejects out-of-order generations; continuous events must not freeze it.
        assert!(schedule.complete(first));
        assert!(schedule.has_pending());
        let newest = schedule.request().unwrap();
        assert_eq!(newest, 10_001);
        assert!(schedule.complete(newest));
        assert!(!schedule.has_pending());
        assert_eq!(schedule.request(), None);
        assert!(!schedule.complete(first));
    }
}
