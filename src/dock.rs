use crate::window_model::{Identity, WindowKey, WindowModel};
use windows::Win32::{
    Foundation::HWND,
    System::Threading::GetCurrentProcessId,
    UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
        SW_MINIMIZE, SW_RESTORE, SetForegroundWindow, ShowWindowAsync,
    },
};

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

/// Accessible names per slot: the picker, then each group's first real window title with count and state.
pub fn names(items: &[DockItem], model: &WindowModel) -> Vec<String> {
    let mut names = vec!["All windows".to_string()];
    for item in items {
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

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Activate(WindowKey),
    Minimize(WindowKey),
}

/// Taskbar-like click: focus the group, cycle within an already focused group, minimize a lone focused window.
pub fn action(item: &DockItem, foreground: Option<WindowKey>) -> Option<Action> {
    let first = *item.windows.first()?;
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

/// Returns false when the target is gone or Windows refused foreground; never forces focus.
pub fn perform(action: Action) -> bool {
    let (Action::Activate(key) | Action::Minimize(key)) = action;
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
}
