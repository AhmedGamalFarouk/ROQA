use crate::layout::Bounds;

/// One display monitor as observed in a single enumeration pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    /// GDI device name (`\\.\DISPLAYn`): session-local identity. Neither enumeration order nor an
    /// HMONITOR survives a topology change, so neither is used to key surfaces.
    pub device: String,
    pub work: Bounds,
    pub dpi: u32,
}

/// A usable snapshot is non-empty, has positive work areas and DPI and unique identities. It is
/// sorted so enumeration order never causes a rebuild. `None` marks a transient observation
/// (Windows reports these mid-change); the host keeps its surfaces and retries.
pub fn settle(mut observed: Vec<Monitor>) -> Option<Vec<Monitor>> {
    observed.sort_by(|a, b| a.device.cmp(&b.device));
    let valid =
        |m: &Monitor| !m.device.is_empty() && m.work.width > 0 && m.work.height > 0 && m.dpi > 0;
    let unique = observed
        .windows(2)
        .all(|pair| pair[0].device != pair[1].device);
    (!observed.is_empty() && unique && observed.iter().all(valid)).then_some(observed)
}

/// Surface work between two settled snapshots: devices whose surfaces must be released (removed
/// or changed) and monitors that need new surfaces (added or changed). Unchanged monitors keep theirs.
pub fn diff(current: &[Monitor], next: &[Monitor]) -> (Vec<String>, Vec<Monitor>) {
    let release = current
        .iter()
        .filter(|old| !next.contains(old))
        .map(|old| old.device.clone())
        .collect();
    let place = next
        .iter()
        .filter(|new| !current.contains(new))
        .cloned()
        .collect();
    (release, place)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{SurfaceKind, placement};

    fn monitor(device: &str, x: i32, y: i32, dpi: u32) -> Monitor {
        Monitor {
            device: device.into(),
            work: Bounds {
                x,
                y,
                width: 1920,
                height: 1040,
            },
            dpi,
        }
    }

    #[test]
    fn enumeration_order_is_not_identity_and_unchanged_snapshots_do_nothing() {
        let current = settle(vec![monitor("B", -1920, -200, 144), monitor("A", 0, 0, 96)]).unwrap();
        let reordered =
            settle(vec![monitor("A", 0, 0, 96), monitor("B", -1920, -200, 144)]).unwrap();
        assert_eq!(current, reordered);
        assert_eq!(diff(&current, &reordered), (vec![], vec![]));
    }

    #[test]
    fn transient_snapshots_are_retried_not_applied() {
        assert_eq!(settle(vec![]), None);
        let mut empty_work = monitor("A", 0, 0, 96);
        empty_work.work.height = 0;
        assert_eq!(settle(vec![empty_work]), None);
        assert_eq!(settle(vec![monitor("A", 0, 0, 0)]), None);
        assert_eq!(settle(vec![monitor("", 0, 0, 96)]), None);
        assert_eq!(
            settle(vec![monitor("A", 0, 0, 96), monitor("A", 1920, 0, 96)]),
            None
        );
    }

    #[test]
    fn work_area_and_dpi_changes_rebuild_only_that_monitor() {
        let current = settle(vec![monitor("A", 0, 0, 96), monitor("B", -1920, -200, 144)]).unwrap();
        let mut taskbar_moved = current.clone();
        taskbar_moved[0].work.height = 1000;
        taskbar_moved[1].dpi = 192;
        let (release, place) = diff(&current, &taskbar_moved);
        assert_eq!(release, ["A", "B"]);
        assert_eq!(place, taskbar_moved);

        let mut one_changed = current.clone();
        one_changed[1].work.x = -2560;
        let (release, place) = diff(&current, &one_changed);
        assert_eq!(release, ["B"]);
        assert_eq!(place, [one_changed[1].clone()]);
    }

    #[test]
    fn removal_and_addition_touch_only_those_monitors() {
        let current = settle(vec![monitor("A", 0, 0, 96), monitor("B", -1920, -200, 144)]).unwrap();
        let unplugged = settle(vec![monitor("A", 0, 0, 96)]).unwrap();
        assert_eq!(diff(&current, &unplugged), (vec!["B".into()], vec![]));
        assert_eq!(
            diff(&unplugged, &current),
            (vec![], vec![current[1].clone()])
        );
    }

    #[test]
    fn negative_origin_monitors_keep_both_surfaces_inside_their_work_area() {
        for screen in [
            monitor("L", -1920, -200, 144),
            monitor("T", -300, -1080, 120),
        ] {
            for (kind, slots) in [(SurfaceKind::Notch, 0), (SurfaceKind::Dock, 9)] {
                let at = placement(screen.work, screen.dpi, kind, slots);
                let work = screen.work;
                assert!(at.x >= work.x && at.y >= work.y);
                assert!(at.x + at.width <= work.x + work.width);
                assert!(at.y + at.height <= work.y + work.height);
            }
        }
    }
}
