use crate::{
    layout::{Bounds, DockStyle},
    sort,
};

/// One display monitor as observed in a single enumeration pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    /// GDI device name (`\\.\DISPLAYn`): session-local identity. Neither enumeration order nor an
    /// HMONITOR survives a topology change, so neither is used to key surfaces.
    pub device: String,
    pub work: Bounds,
    /// The whole monitor, taskbar included: its size and place in the desktop, for the Settings display rows.
    pub rect: Bounds,
    pub dpi: u32,
    /// Pixels a taskbar that hides at the bottom covers when it slides in; 0 when it shows or sits elsewhere.
    pub taskbar: i32,
}
impl Monitor {
    /// Where a dock of `style` sits: a floating one stays above the taskbar that slides in over the bottom edge.
    pub fn dock_room(&self, style: DockStyle) -> Bounds {
        let lift = if style == DockStyle::Floating {
            self.taskbar
        } else {
            0
        };
        Bounds {
            height: (self.work.height - lift).max(1),
            ..self.work
        }
    }
}

/// A usable snapshot is non-empty, has positive work areas and DPI and unique identities. It is
/// sorted so enumeration order never causes a rebuild. `None` marks a transient observation
/// (Windows reports these mid-change); the host keeps its surfaces and retries.
pub fn settle(mut observed: Vec<Monitor>) -> Option<Vec<Monitor>> {
    sort::by(&mut observed, |a, b| a.device.cmp(&b.device));
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

/// A window that spans a whole monitor, borders included, hides the shell UI on it when Windows
/// also reports a full-screen app. With an auto-hidden taskbar, a maximized window can span the
/// monitor without being full screen; even an exact match needs a report for that window.
pub fn covers(window: Bounds, monitor: Bounds, maximized: bool, shell_confirmed: bool) -> bool {
    if maximized {
        return shell_confirmed && window == monitor;
    }
    window.x <= monitor.x
        && window.y <= monitor.y
        && window.x + window.width >= monitor.x + monitor.width
        && window.y + window.height >= monitor.y + monitor.height
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
            rect: Bounds {
                x,
                y,
                width: 1920,
                height: 1080,
            },
            dpi,
            taskbar: 0,
        }
    }

    #[test]
    fn work_area_resize_replaces_the_notch_at_a_scale_that_fits() {
        let primary = monitor("primary", 0, 0, 96);
        let secondary = monitor("secondary", 1920, 0, 192);
        let mut narrowed = secondary.clone();
        narrowed.work.width = 1024;
        let old_dpi = crate::layout::notch_dpi(secondary.work, secondary.dpi, 2.25);
        let (release, place) = diff(&[primary.clone(), secondary], &[primary, narrowed.clone()]);
        assert_eq!(release, ["secondary"]);
        assert_eq!(place, [narrowed.clone()]);
        let new_dpi = crate::layout::notch_dpi(narrowed.work, narrowed.dpi, 2.25);
        assert!(new_dpi < old_dpi);
        let notch = placement(narrowed.work, new_dpi, SurfaceKind::Notch, 0);
        let rail = crate::layout::Rail {
            count: 7,
            active: 6,
            expansion: [0.0; 7],
        };
        let rail = crate::layout::rail_bounds(notch.width, new_dpi, 1.0, true, rail).unwrap();
        assert!(rail.x >= 0 && rail.x + rail.width <= notch.width);
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
    fn full_screen_means_covering_the_whole_monitor_including_negative_origins() {
        let left = monitor("L", -1920, -200, 96).work;
        assert!(covers(left, left, false, false));
        let oversized = Bounds {
            x: left.x - 8,
            y: left.y - 8,
            width: left.width + 16,
            height: left.height + 16,
        };
        assert!(covers(oversized, left, false, false));
        // A maximized browser on a monitor whose taskbar hides spans it too, frame and all: not full screen.
        assert!(!covers(oversized, left, true, true));
        // Full screen that leaves its window maximized fills the monitor exactly.
        assert!(covers(left, left, true, true));
        assert!(!covers(left, left, true, false));
        let mut short = left;
        short.height -= 1;
        assert!(!covers(short, left, false, false));
        let mut shifted = left;
        shifted.x += 1;
        assert!(!covers(shifted, left, false, false));
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
