use crate::pins::{self, Pins};
use crate::window_model::{Identity, WindowKey, WindowModel};
use windows::Win32::{
    Foundation::HWND,
    System::Threading::GetCurrentProcessId,
    UI::{
        Shell::{SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW, ShellExecuteExW},
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
            SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL, SetForegroundWindow, ShowWindowAsync,
        },
    },
};
use windows::core::{HSTRING, PCWSTR};

/// Dock geometry in DIPs. Slot 0 is the "all windows" picker; app groups follow.
pub const SLOT_DIPS: i32 = 56;
pub const PAD_DIPS: i32 = 8;
pub const ICON_DIPS: i32 = 40;
pub const HEIGHT_DIPS: i32 = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockItem {
    pub identity: Identity,
    pub executable: Option<String>,
    pub windows: Vec<WindowKey>,
    pub active: bool,
}

/// One item per real catalog group, in stable identity order. No placeholder rows.
pub fn items(model: &WindowModel) -> Vec<DockItem> {
    let mut items: Vec<DockItem> = Vec::new();
    let mut windows: Vec<_> = model.windows.iter().collect();
    windows.sort_by(|a, b| (&a.identity, a.key).cmp(&(&b.identity, b.key)));
    for window in windows {
        match items.last_mut() {
            Some(item) if item.identity == window.identity => {
                item.windows.push(window.key);
                item.active |= window.foreground;
                if item.executable.is_none() {
                    item.executable.clone_from(&window.executable);
                }
            }
            _ => items.push(DockItem {
                identity: window.identity.clone(),
                executable: window.executable.clone(),
                windows: vec![window.key],
                active: window.foreground,
            }),
        }
    }
    items
}

/// Pins first, in pin order (running group or a launcher with no windows), then unpinned groups in `items` order.
/// An `Executable` pin also claims a group whose AUMID identity has the same exe, so a pin survives that upgrade.
/// With no pins this equals `items(model)`.
pub fn items_with_pins(model: &WindowModel, pins: &Pins) -> Vec<DockItem> {
    let mut running: Vec<Option<DockItem>> = items(model).into_iter().map(Some).collect();
    let find = |running: &[Option<DockItem>], matches: &dyn Fn(&DockItem) -> bool| {
        running
            .iter()
            .position(|item| item.as_ref().is_some_and(matches))
    };
    let mut merged = Vec::with_capacity(pins.items().len() + running.len());
    for pin in pins.items() {
        let found = find(&running, &|item| item.identity == *pin).or_else(|| match pin {
            Identity::Executable(path) => {
                find(&running, &|item| item.executable.as_deref() == Some(path))
            }
            _ => None,
        });
        merged.push(
            found
                .and_then(|at| running[at].take())
                .unwrap_or_else(|| DockItem {
                    identity: pin.clone(),
                    executable: match pin {
                        Identity::Executable(path) => Some(path.clone()),
                        _ => None,
                    },
                    windows: Vec::new(),
                    active: false,
                }),
        );
    }
    merged.extend(running.into_iter().flatten());
    merged
}

/// Accessible names per slot: the picker, then each group's first real window title with count and state.
pub fn names(items: &[DockItem], model: &WindowModel) -> Vec<String> {
    let mut names = vec!["All windows".to_string()];
    for item in items {
        if item.windows.is_empty() {
            names.push(format!(
                "{}, pinned, not running",
                pins::display_name(&item.identity)
            ));
            continue;
        }
        let title = item
            .windows
            .first()
            .and_then(|key| model.windows.iter().find(|window| window.key == *key))
            .map_or("Window", |window| window.title.as_str());
        let mut name = title.to_string();
        if item.windows.len() > 1 {
            name.push_str(&format!(", {} windows", item.windows.len()));
        }
        if item.active {
            name.push_str(", active");
        }
        names.push(name);
    }
    names
}

pub fn width_dips(slots: usize) -> i32 {
    PAD_DIPS * 2 + SLOT_DIPS * slots.max(1) as i32
}

pub fn px(dips: i32, dpi: u32) -> i32 {
    ((i64::from(dips) * i64::from(dpi.max(1)) + 48) / 96) as i32
}

/// Slots that fit inside the realized window width; the rest stay reachable through the picker.
pub fn visible_slots(width_px: i32, dpi: u32, slots: usize) -> usize {
    let fit = (width_px - px(PAD_DIPS, dpi) * 2) / px(SLOT_DIPS, dpi).max(1);
    slots.min(fit.max(0) as usize)
}

pub fn slot_at(x_px: i32, width_px: i32, dpi: u32, slots: usize) -> Option<usize> {
    let offset = x_px - px(PAD_DIPS, dpi);
    if offset < 0 {
        return None;
    }
    let slot = (offset / px(SLOT_DIPS, dpi).max(1)) as usize;
    (slot < visible_slots(width_px, dpi, slots)).then_some(slot)
}

/// Pointer travel (DIPs) before a press on a pinned slot becomes a reorder drag instead of a click.
pub const DRAG_DIPS: i32 = 6;

pub fn drag_started(origin_x: i32, x: i32, dpi: u32) -> bool {
    (x - origin_x).abs() >= px(DRAG_DIPS, dpi)
}

/// Pin index a drag released at `x_px` drops on: the slot under the pointer (clamped to the visible pins, so
/// releasing beyond either end drops on the first or last), or `None` with no visible pin. Item `i` is pin `i`.
pub fn drop_index(x_px: i32, width_px: i32, dpi: u32, slots: usize, pins: usize) -> Option<usize> {
    let last = visible_slots(width_px, dpi, slots)
        .saturating_sub(1)
        .min(pins)
        .checked_sub(1)?;
    let slot = (x_px - px(PAD_DIPS, dpi)).div_euclid(px(SLOT_DIPS, dpi).max(1));
    Some((slot - 1).clamp(0, last as i32) as usize)
}

/// The pin that represents `item`, if any: its identity, or the executable pin that claimed it.
pub fn pinned_as(item: &DockItem, pins: &Pins) -> Option<Identity> {
    let exe = item.executable.clone().map(Identity::Executable);
    [Some(item.identity.clone()), exe]
        .into_iter()
        .flatten()
        .find(|identity| pins.position(identity).is_some())
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Activate(WindowKey),
    Minimize(WindowKey),
    /// Shell target of a pinned app with no window; `launch` runs it (`perform` only handles window actions).
    Launch(String),
}

/// Taskbar-like click: focus the group, cycle within an already focused group, minimize a lone focused window.
/// A pinned item with no windows launches instead.
pub fn action(item: &DockItem, foreground: Option<WindowKey>) -> Option<Action> {
    let Some(&first) = item.windows.first() else {
        return pins::launch_target(&item.identity).map(Action::Launch);
    };
    let Some(position) = foreground.and_then(|key| item.windows.iter().position(|w| *w == key))
    else {
        return Some(Action::Activate(first));
    };
    if item.windows.len() == 1 {
        Some(Action::Minimize(first))
    } else {
        Some(Action::Activate(
            item.windows[(position + 1) % item.windows.len()],
        ))
    }
}

pub fn foreground_key() -> Option<WindowKey> {
    // SAFETY: scalar HWND/PID metadata; no cross-process dereference or messages.
    unsafe {
        let window = GetForegroundWindow();
        let mut process = 0;
        GetWindowThreadProcessId(window, Some(&mut process));
        (!window.0.is_null() && process != 0).then_some(WindowKey {
            handle: window.0 as usize,
            process,
        })
    }
}

/// Revalidates a catalog key immediately before acting on it: same PID, still a visible top-level window, not ROQA.
pub fn valid_target(key: WindowKey) -> bool {
    // SAFETY: only scalar metadata checks on a transient foreign HWND, immediately before the action.
    unsafe {
        let window = HWND(key.handle as *mut _);
        let mut process = 0;
        GetWindowThreadProcessId(window, Some(&mut process));
        process == key.process
            && process != GetCurrentProcessId()
            && IsWindow(Some(window)).as_bool()
            && IsWindowVisible(window).as_bool()
    }
}

/// After an explicit dismissal, gives the foreground back to `to` if ROQA's `from` window still
/// holds it and `to` is still a valid target.
pub fn restore_foreground(from: HWND, to: Option<WindowKey>) {
    // SAFETY: scalar foreground query and request; `to` is revalidated immediately before.
    unsafe {
        if GetForegroundWindow() == from
            && let Some(key) = to.filter(|key| valid_target(*key))
        {
            let _ = SetForegroundWindow(HWND(key.handle as *mut _));
        }
    }
}

/// Starts a not-running pin through the shell, on the user's click only. No UI on failure (a missing target
/// would otherwise open a modal error box on the host thread); false means it did not start.
pub fn launch(target: &str) -> bool {
    let file = HSTRING::from(target);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI,
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `info` and `file` outlive the call; no process handle is requested, so nothing is left to close.
    unsafe { ShellExecuteExW(&mut info).is_ok() }
}

/// Returns false when the target is gone or Windows refused foreground; never forces focus.
pub fn perform(action: Action) -> bool {
    let (Action::Activate(key) | Action::Minimize(key)) = action else {
        return false; // Launch goes through `launch`.
    };
    if !valid_target(key) {
        return false;
    }
    let window = HWND(key.handle as *mut _);
    // SAFETY: revalidated foreign HWND; asynchronous show calls never block on its message procedure.
    unsafe {
        match action {
            Action::Minimize(_) => ShowWindowAsync(window, SW_MINIMIZE).as_bool(),
            Action::Activate(_) => {
                if IsIconic(window).as_bool() {
                    let _ = ShowWindowAsync(window, SW_RESTORE);
                }
                SetForegroundWindow(window).as_bool()
            }
            Action::Launch(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_model::WindowRecord;

    fn record(handle: usize, identity: Identity, foreground: bool) -> WindowRecord {
        WindowRecord {
            key: WindowKey {
                handle,
                process: handle as u32,
            },
            title: format!("w{handle}"),
            identity,
            executable: Some(format!("c:\\{handle}.exe")),
            minimized: false,
            foreground,
        }
    }
    fn key(handle: usize) -> WindowKey {
        WindowKey {
            handle,
            process: handle as u32,
        }
    }

    #[test]
    fn groups_real_windows_by_identity_in_stable_order() {
        let app = Identity::Application("Contoso.Mail".into());
        let exe = Identity::Executable("c:\\editor.exe".into());
        let model = WindowModel {
            generation: 1,
            windows: vec![
                record(9, exe.clone(), false),
                record(3, app.clone(), false),
                record(5, exe.clone(), true),
                record(7, Identity::Unavailable(7), false),
            ],
            incomplete: None,
        };
        let items = items(&model);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].identity, app);
        assert_eq!(items[1].windows, vec![key(5), key(9)]);
        assert!(items[1].active && !items[0].active);
        assert_eq!(items[2].identity, Identity::Unavailable(7));
        assert!(super::items(&WindowModel::default()).is_empty());
        assert_eq!(
            names(&items, &model),
            vec!["All windows", "w3", "w5, 2 windows, active", "w7"]
        );
    }

    fn pins_of(identities: &[Identity]) -> Pins {
        let mut pins = Pins::default();
        for identity in identities {
            pins.add(identity.clone()).unwrap();
        }
        pins
    }

    fn sample_model() -> WindowModel {
        WindowModel {
            generation: 1,
            windows: vec![
                record(9, Identity::Executable("c:\\9.exe".into()), false),
                record(3, Identity::Application("Contoso.Mail".into()), true),
                record(5, Identity::Executable("c:\\5.exe".into()), false),
                record(7, Identity::Unavailable(7), false),
            ],
            incomplete: None,
        }
    }

    #[test]
    fn empty_pins_equal_todays_items() {
        let model = sample_model();
        assert_eq!(items_with_pins(&model, &Pins::default()), items(&model));
        assert!(items_with_pins(&WindowModel::default(), &Pins::default()).is_empty());
    }

    #[test]
    fn merge_orders_pinned_first_then_running_in_stable_order() {
        let model = sample_model();
        let missing = Identity::Executable("c:\\gone.exe".into());
        let pinned = [
            Identity::Executable("c:\\9.exe".into()), // running, pinned
            missing.clone(),                          // pinned only
            Identity::Application("Contoso.Mail".into()),
        ];
        let merged = items_with_pins(&model, &pins_of(&pinned));
        let ids: Vec<_> = merged.iter().map(|i| i.identity.clone()).collect();
        // Pin order, then unpinned running (5.exe, Unavailable) in the old identity order.
        assert_eq!(
            ids,
            [
                pinned[0].clone(),
                missing.clone(),
                pinned[2].clone(),
                Identity::Executable("c:\\5.exe".into()),
                Identity::Unavailable(7)
            ]
        );
        assert_eq!(merged[0].windows, vec![key(9)]);
        assert!(merged[1].windows.is_empty() && !merged[1].active);
        assert_eq!(merged[1].executable.as_deref(), Some("c:\\gone.exe"));
        assert!(merged[2].active);
        // Stable: the same inputs give the same order, and a window churn does not reorder pins.
        assert_eq!(merged, items_with_pins(&model, &pins_of(&pinned)));
        let mut fewer = sample_model();
        fewer.windows.retain(|w| w.key != key(9));
        let ids: Vec<_> = items_with_pins(&fewer, &pins_of(&pinned))
            .iter()
            .map(|i| i.identity.clone())
            .collect();
        assert_eq!(ids[..3], pinned);
    }

    #[test]
    fn executable_pin_claims_a_group_that_gained_an_aumid() {
        let mut model = WindowModel::default();
        model
            .windows
            .push(record(4, Identity::Application("Vendor.App".into()), false));
        model.windows[0].executable = Some("c:\\vendor.exe".into());
        let merged = items_with_pins(
            &model,
            &pins_of(&[Identity::Executable("C:\\Vendor.exe".into())]),
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].windows, vec![key(4)]);
        // An exact identity match wins and a group is claimed once.
        model.windows.push(record(
            6,
            Identity::Executable("c:\\vendor.exe".into()),
            false,
        ));
        let pins = pins_of(&[Identity::Executable("c:\\vendor.exe".into())]);
        let merged = items_with_pins(&model, &pins);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].windows, vec![key(6)]);
        assert_eq!(merged[1].windows, vec![key(4)]);
    }

    #[test]
    fn launcher_items_get_names_and_a_launch_action() {
        let pins = pins_of(&[
            Identity::Application("Contoso.Mail_abc!App".into()),
            Identity::Executable("c:\\tools\\edit.exe".into()),
        ]);
        let model = WindowModel::default();
        let merged = items_with_pins(&model, &pins);
        assert_eq!(
            names(&merged, &model),
            [
                "All windows",
                "Contoso.Mail, pinned, not running",
                "edit, pinned, not running"
            ]
        );
        assert_eq!(
            action(&merged[0], None),
            Some(Action::Launch(
                "shell:AppsFolder\\Contoso.Mail_abc!App".into()
            ))
        );
        assert_eq!(
            action(&merged[1], Some(key(1))),
            Some(Action::Launch("c:\\tools\\edit.exe".into()))
        );
        assert!(!perform(Action::Launch("c:\\tools\\edit.exe".into()))); // S4 wires the launch
    }

    #[test]
    fn click_activates_cycles_and_minimizes_like_a_taskbar() {
        let item = |handles: &[usize]| DockItem {
            identity: Identity::Unavailable(0),
            executable: None,
            windows: handles.iter().map(|h| key(*h)).collect(),
            active: false,
        };
        assert_eq!(
            action(&item(&[1, 2]), Some(key(8))),
            Some(Action::Activate(key(1)))
        );
        assert_eq!(action(&item(&[1, 2]), None), Some(Action::Activate(key(1))));
        assert_eq!(
            action(&item(&[1, 2, 3]), Some(key(3))),
            Some(Action::Activate(key(1)))
        );
        assert_eq!(
            action(&item(&[1]), Some(key(1))),
            Some(Action::Minimize(key(1)))
        );
        assert_eq!(action(&item(&[]), None), None);
    }

    #[test]
    fn slot_hit_testing_matches_scaled_geometry_and_clipping() {
        // 150%: pad 12px, slot 84px. Three slots need 276px.
        assert_eq!(px(width_dips(3), 144), 276);
        assert_eq!(slot_at(11, 276, 144, 3), None);
        assert_eq!(slot_at(12, 276, 144, 3), Some(0));
        assert_eq!(slot_at(95, 276, 144, 3), Some(0));
        assert_eq!(slot_at(96, 276, 144, 3), Some(1));
        assert_eq!(slot_at(263, 276, 144, 3), Some(2));
        assert_eq!(slot_at(264, 276, 144, 3), None);
        // Work area narrower than the content: only fully visible slots are hit targets.
        assert_eq!(visible_slots(200, 144, 3), 2);
        assert_eq!(slot_at(190, 200, 144, 3), None);
        assert_eq!(width_dips(0), width_dips(1));
    }

    #[test]
    fn drag_starts_only_past_the_threshold_at_any_scale() {
        assert!(!drag_started(100, 105, 96) && drag_started(100, 106, 96));
        assert!(!drag_started(100, 95, 96) && drag_started(100, 94, 96));
        // 150%: 9 px.
        assert!(!drag_started(100, 108, 144) && drag_started(100, 109, 144));
        assert!(!drag_started(0, 0, 96));
    }

    #[test]
    fn drop_index_uses_the_dock_slots_and_clamps_to_the_pins() {
        // 96 dpi: pad 8, slot 56; slot 0 is the picker, so item i is slot i + 1 at x = 8 + 56 * (i + 1).
        let at = |x, slots, pins| drop_index(x, px(width_dips(slots), 96), 96, slots, pins);
        // 4 pins among 8 slots (picker, 4 pins, 3 running): slots 1..=4 are pins.
        assert_eq!(at(8 + 56, 8, 4), Some(0));
        assert_eq!(at(8 + 56 * 3 + 55, 8, 4), Some(2));
        assert_eq!(at(8 + 56 * 4, 8, 4), Some(3));
        // On the picker, left of the bar, or beyond the pins: first and last pin.
        assert_eq!(at(8 + 10, 8, 4), Some(0));
        assert_eq!(at(-300, 8, 4), Some(0));
        assert_eq!(at(8 + 56 * 7, 8, 4), Some(3));
        assert_eq!(at(10_000, 8, 4), Some(3));
        // No pins: nothing to drop on.
        assert_eq!(at(70, 8, 0), None);
        // Clipped bar (width for 3 slots, so only 2 pins visible): the drop never lands on a hidden pin.
        assert_eq!(drop_index(10_000, px(width_dips(3), 96), 96, 8, 4), Some(1));
        // Agrees with `slot_at`: the slot a click would hit is the slot a drop lands on.
        let width = px(width_dips(8), 96);
        for x in 0..width {
            if let Some(slot @ 1..=4) = slot_at(x, width, 96, 8) {
                assert_eq!(drop_index(x, width, 96, 8, 4), Some(slot - 1), "x {x}");
            }
        }
        // 150%: pad 12, slot 84.
        assert_eq!(
            drop_index(12 + 84 * 2, px(width_dips(5), 144), 144, 5, 3),
            Some(1)
        );
    }

    #[test]
    fn pin_toggle_target_follows_the_claiming_pin() {
        let running = |identity, exe: &str| DockItem {
            identity,
            executable: Some(exe.into()),
            windows: vec![key(1)],
            active: false,
        };
        let aumid = running(Identity::Application("Vendor.App".into()), r"c:\vendor.exe");
        let mut pins = Pins::default();
        assert_eq!(pinned_as(&aumid, &pins), None);
        // An executable pin that claimed this group is the one to remove.
        pins.add(Identity::Executable(r"c:\vendor.exe".into()))
            .unwrap();
        assert_eq!(
            pinned_as(&aumid, &pins),
            Some(Identity::Executable(r"c:\vendor.exe".into()))
        );
        // The exact identity wins when both exist.
        pins.add(aumid.identity.clone()).unwrap();
        assert_eq!(pinned_as(&aumid, &pins), Some(aumid.identity.clone()));
        // A pin-only item and an unpinnable one.
        let gone = DockItem {
            identity: Identity::Application("Gone".into()),
            executable: None,
            windows: vec![],
            active: false,
        };
        assert_eq!(pinned_as(&gone, &pins), None);
        let unavailable = running(Identity::Unavailable(3), r"c:\other.exe");
        assert_eq!(pinned_as(&unavailable, &pins), None);
    }
}
