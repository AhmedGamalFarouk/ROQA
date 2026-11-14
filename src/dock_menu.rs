//! Dock context menu (features B3): right-click on a slot opens a small owned popup above it with the
//! actions that apply to that slot. It is not `TrackPopupMenu`, which is modal and would stop the host
//! loop. The popup exists only while open, takes the foreground, and ends on a
//! press elsewhere, losing the foreground, its slot changing or a full-screen app; closed, it costs nothing.
use crate::{
    dock::{self, DockItem},
    edge::{ListRow, list_layout, row_at, row_rect},
    graphics::{Canvas, Graphics, ListScene},
    icons::IconPixels,
    layout::Bounds,
    native_window::{self, NativeWindow},
    pins::{self, Pins},
    theme::Palette,
    window_model::{Identity, WindowKey},
};
use std::sync::Arc;
use windows::{
    Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            MSG, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_RBUTTONDOWN,
            WM_RBUTTONUP,
        },
    },
    core::Result,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    NewInstance,
    Close(WindowKey),
    CloseAll,
    Pin,
    Unpin,
    RemoveShelf,
    UndoShelf,
    /// The app menu (right-click on the notch or the dock outside an app slot).
    Settings,
    Quit,
    /// A minimized window in the chevron's list; its row shows the window's title (`Spec::titles`).
    Restore(WindowKey),
}
impl Item {
    pub fn label(self) -> &'static str {
        match self {
            Item::NewInstance => "New instance",
            Item::Close(_) => "Close window",
            Item::CloseAll => "Close all windows",
            Item::Pin => "Pin to dock",
            Item::Unpin => "Unpin from dock",
            Item::RemoveShelf => "Remove from Shelf",
            Item::UndoShelf => "Undo removal",
            Item::Settings => "Settings",
            Item::Quit => "Quit ROQA",
            Item::Restore(_) => "Window",
        }
    }
}

/// The actions that apply to a dock slot: New instance when the app can be launched, Close window only
/// while it has windows (a pinned app that is not running has none), and Pin or Unpin.
pub fn items(item: &DockItem, pins: &Pins) -> Vec<Item> {
    let launchable = pins::launch_target(&item.identity).is_some();
    let mut list = Vec::with_capacity(3);
    if launchable {
        list.push(Item::NewInstance);
    }
    match item.windows.as_slice() {
        [] => {}
        [key] => list.push(Item::Close(*key)),
        _ => list.push(Item::CloseAll),
    }
    if dock::pinned_as(item, pins).is_some() {
        list.push(Item::Unpin);
    } else if launchable {
        list.push(Item::Pin);
    }
    list
}

/// Whether a menu opened for `identity` with `shown` actions still describes `now`, the dock's item in
/// that slot. False once the slot holds another app or its actions changed (its last window closed, a pin
/// changed), which ends the menu.
pub fn describes(
    identity: &Identity,
    shown: &[Item],
    windows: &[WindowKey],
    now: Option<&DockItem>,
    pins: &Pins,
) -> bool {
    now.is_some_and(|now| {
        now.identity == *identity && now.windows == windows && shown == items(now, pins)
    })
}

/// Where the menu window goes (physical pixels): centred over `slot_x`, its bottom edge on the dock's top
/// (the window's transparent margin is the gap), kept inside the work area.
pub fn place(slot_x: i32, dock: Bounds, work: Bounds, size: (i32, i32)) -> Bounds {
    let (width, height) = size;
    Bounds {
        x: (slot_x - width / 2).clamp(work.x, (work.x + work.width - width).max(work.x)),
        y: (dock.y - height).max(work.y),
        width,
        height,
    }
}

/// The app menu's actions, offered where no dock app slot was clicked.
pub const APP_ITEMS: [Item; 2] = [Item::Settings, Item::Quit];

/// Where an app menu opens below the notch: centred on the pointer column, its top on `bottom`, inside the work area.
pub fn place_below(x: i32, bottom: i32, work: Bounds, size: (i32, i32)) -> Bounds {
    let (width, height) = size;
    Bounds {
        x: (x - width / 2).clamp(work.x, (work.x + work.width - width).max(work.x)),
        y: bottom.min((work.y + work.height - height).max(work.y)),
        width,
        height,
    }
}

/// What the host does with a message offered to the menu.
pub enum Event {
    /// Not the menu's business.
    Ignored,
    Consumed,
    /// Close it. A press that was on another ROQA window still goes on to that window.
    Dismiss,
    Choose(Item),
}

/// What to open: the dock slot and what it holds.
pub struct Spec {
    pub dock: HWND,
    pub slot: usize,
    pub identity: Identity,
    pub windows: Vec<WindowKey>,
    pub items: Vec<Item>,
    pub dock_bounds: Bounds,
    pub work: Bounds,
    pub dpi: u32,
    pub app: String,
    /// Row text and icon per item when the labels are not fixed (the minimized list); empty otherwise.
    pub titles: Vec<String>,
    pub icons: Vec<Option<Arc<IconPixels>>>,
    /// The screen column the menu centres on (a slot's centre, or the pointer), and `Some(bottom)` to open it
    /// below that screen row (the notch) instead of above the dock.
    pub x: i32,
    pub below: Option<i32>,
    /// The window that had the foreground, which gets it back when the menu ends.
    pub return_to: Option<WindowKey>,
}

pub struct Menu {
    canvas: Canvas,
    window: NativeWindow,
    dock: HWND,
    pub slot: usize,
    pub identity: Identity,
    pub windows: Vec<WindowKey>,
    items: Vec<Item>,
    titles: Vec<String>,
    icons: Vec<Option<Arc<IconPixels>>>,
    dpi: u32,
    selected: Option<usize>,
    drawn: Option<Option<usize>>,
    pub return_to: Option<WindowKey>,
}
/// A row's text: its window title when it has one, else the action's fixed label.
fn row_label<'a>(items: &'a [Item], titles: &'a [String], index: usize) -> &'a str {
    titles
        .get(index)
        .map_or_else(|| items[index].label(), String::as_str)
}

fn point(lparam: windows::Win32::Foundation::LPARAM) -> (i32, i32) {
    (
        i32::from((lparam.0 & 0xffff) as u16 as i16),
        i32::from((lparam.0 >> 16) as u16 as i16),
    )
}

impl Menu {
    /// Creates, paints and shows the popup (not yet the foreground: the host asks for that).
    pub fn open(graphics: &Graphics, palette: Palette, spec: Spec) -> Result<Self> {
        let dpi = spec.dpi;
        let (size, _) = list_layout(dpi, spec.items.len());
        let slot_x = spec.x;
        let bounds = match spec.below {
            Some(bottom) => place_below(slot_x, bottom, spec.work, size),
            None => place(slot_x, spec.dock_bounds, spec.work, size),
        };
        let window = NativeWindow::create_menu(bounds, &format!("{} menu", spec.app))?;
        let surface = Bounds {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        let canvas = Canvas::create(graphics, window.0, surface, palette)?;
        let mut menu = Self {
            canvas,
            window,
            dock: spec.dock,
            slot: spec.slot,
            identity: spec.identity,
            windows: spec.windows,
            items: spec.items,
            titles: spec.titles,
            icons: spec.icons,
            dpi,
            selected: None,
            drawn: None,
            return_to: spec.return_to,
        };
        menu.paint()?;
        menu.window.show();
        Ok(menu)
    }
    pub fn hwnd(&self) -> HWND {
        self.window.0
    }
    pub fn dock(&self) -> HWND {
        self.dock
    }
    pub fn items(&self) -> &[Item] {
        &self.items
    }
    /// Repaints only when the highlighted row changed.
    pub fn paint(&mut self) -> Result<()> {
        if self.drawn == Some(self.selected) {
            return Ok(());
        }
        let (_, panel) = list_layout(self.dpi, self.items.len());
        let rows: Vec<ListRow> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, _)| ListRow {
                name: row_label(&self.items, &self.titles, index),
                current: false,
                icon: self.icons.get(index).and_then(|icon| icon.as_deref()),
                rect: row_rect(self.dpi, panel, index),
            })
            .collect();
        self.canvas.paint_list(&ListScene {
            dpi: self.dpi,
            panel,
            rows: &rows,
            hover: self.selected,
        })?;
        self.drawn = Some(self.selected);
        Ok(())
    }
    /// One message: mouse input for the menu's window, losing the foreground, and a button press on any
    /// other ROQA window.
    pub fn message(&mut self, message: &MSG) -> Event {
        if message.hwnd != self.window.0 {
            let press = matches!(
                message.message,
                WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
            );
            return if press {
                Event::Dismiss
            } else {
                Event::Ignored
            };
        }
        let at = |message: &MSG| {
            let (x, y) = point(message.lParam);
            row_at(self.dpi, self.items.len(), x, y)
        };
        match message.message {
            WM_MOUSEMOVE => self.selected = at(message),
            WM_LBUTTONUP => {
                return at(message).map_or(Event::Consumed, |row| Event::Choose(self.items[row]));
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_RBUTTONUP => {}
            native_window::DEACTIVATED => return Event::Dismiss,
            _ => return Event::Ignored,
        }
        Event::Consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_of(handle: usize) -> WindowKey {
        WindowKey {
            handle,
            process: handle as u32,
        }
    }
    fn exe(path: &str) -> Identity {
        Identity::Executable(path.into())
    }
    fn dock_item(identity: Identity, windows: &[usize]) -> DockItem {
        DockItem {
            executable: match &identity {
                Identity::Executable(path) => Some(path.clone()),
                _ => None,
            },
            identity,
            windows: windows.iter().map(|&h| key_of(h)).collect(),
            active: false,
            badge: crate::dock::Badge::None,
        }
    }
    fn pinned(identity: &Identity) -> Pins {
        let mut pins = Pins::default();
        pins.add(identity.clone()).unwrap();
        pins
    }

    #[test]
    fn actions_follow_what_the_slot_is() {
        let app = exe(r"c:\app.exe");
        let none = Pins::default();
        // Running (2 windows), unpinned: CloseAll is offered because it has multiple windows.
        let running2 = dock_item(app.clone(), &[1, 2]);
        assert_eq!(
            items(&running2, &none),
            [Item::NewInstance, Item::CloseAll, Item::Pin]
        );
        // Running (1 window), unpinned: Close is offered.
        let running1 = dock_item(app.clone(), &[1]);
        assert_eq!(
            items(&running1, &none),
            [Item::NewInstance, Item::Close(key_of(1)), Item::Pin]
        );
        // Running (2 windows) and pinned: Unpin replaces Pin.
        assert_eq!(
            items(&running2, &pinned(&app)),
            [Item::NewInstance, Item::CloseAll, Item::Unpin]
        );
        // A pinned app that is not running has no window to close.
        let idle = dock_item(app.clone(), &[]);
        assert_eq!(
            items(&idle, &pinned(&app)),
            [Item::NewInstance, Item::Unpin]
        );
        // A packaged app launches through its AUMID; one with no usable identity can only close.
        let packaged = dock_item(Identity::Application("Contoso.Mail!App".into()), &[3]);
        assert_eq!(
            items(&packaged, &none),
            [Item::NewInstance, Item::Close(key_of(3)), Item::Pin]
        );
        let unknown = dock_item(Identity::Unavailable(9), &[9]);
        assert_eq!(items(&unknown, &none), [Item::Close(key_of(9))]);
    }

    #[test]
    fn the_menu_sits_above_the_slot_inside_the_work_area() {
        let work = Bounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        let dock = Bounds {
            x: 700,
            y: 950,
            width: 400,
            height: 64,
        };
        let size = (266, 120);
        let at = |slot_x| place(slot_x, dock, work, size);
        let middle = at(900);
        assert_eq!((middle.x, middle.y), (900 - 133, 950 - 120));
        assert_eq!((middle.width, middle.height), size);
        // Clamped to the work area at both ends, and to its top when the dock leaves no room.
        assert_eq!(at(5).x, 0);
        assert_eq!(at(1915).x, 1920 - 266);
        assert_eq!(place(900, Bounds { y: 60, ..dock }, work, size).y, 0);
        // A work area narrower than the menu pins it to the left edge.
        let narrow = Bounds { width: 100, ..work };
        assert_eq!(place(50, dock, narrow, size).x, 0);
    }

    #[test]
    fn the_app_menu_opens_below_the_notch_inside_the_work_area() {
        let work = Bounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        let size = (266, 90);
        let at = |x, bottom| place_below(x, bottom, work, size);
        assert_eq!((at(960, 36).x, at(960, 36).y), (960 - 133, 36));
        assert_eq!(at(5, 36).x, 0);
        assert_eq!(at(1915, 36).x, 1920 - 266);
        assert_eq!(at(960, 1030).y, 1040 - 90, "kept on screen");
        assert_eq!(APP_ITEMS, [Item::Settings, Item::Quit]);
        assert!(APP_ITEMS.iter().all(|item| !item.label().is_empty()));
    }

    #[test]
    fn the_menu_ends_when_its_slot_no_longer_matches() {
        let app = exe(r"c:\app.exe");
        let pins = Pins::default();
        let shown = items(&dock_item(app.clone(), &[1]), &pins);
        let same = dock_item(app.clone(), &[1]);
        let expanded = dock_item(app.clone(), &[1, 7]);
        let replaced = dock_item(app.clone(), &[7]);
        assert!(!describes(
            &app,
            &shown,
            &[key_of(1)],
            Some(&expanded),
            &pins
        ));
        assert!(!describes(
            &app,
            &shown,
            &[key_of(1)],
            Some(&replaced),
            &pins
        ));
        assert!(describes(&app, &shown, &[key_of(1)], Some(&same), &pins));
        assert!(!describes(&app, &shown, &[key_of(1)], None, &pins));
        let other = dock_item(exe(r"c:\other.exe"), &[1]);
        assert!(!describes(&app, &shown, &[key_of(1)], Some(&other), &pins));
        let closed = dock_item(app.clone(), &[]);
        assert!(!describes(&app, &shown, &[key_of(1)], Some(&closed), &pins));
        assert!(!describes(
            &app,
            &shown,
            &[key_of(1)],
            Some(&same),
            &pinned(&app)
        ));
    }

    #[test]
    fn a_close_all_menu_expires_when_the_group_changes_but_keeps_multiple_windows() {
        let app = exe(r"c:\app.exe");
        let pins = Pins::default();
        let original = dock_item(app.clone(), &[1, 2]);
        let shown = items(&original, &pins);
        let expanded = dock_item(app.clone(), &[1, 2, 3]);
        let replaced = dock_item(app.clone(), &[1, 3]);
        for changed in [&expanded, &replaced] {
            assert_eq!(shown, items(changed, &pins));
            assert!(!describes(
                &app,
                &shown,
                &original.windows,
                Some(changed),
                &pins
            ));
        }
        assert!(describes(
            &app,
            &shown,
            &original.windows,
            Some(&original),
            &pins
        ));
    }

    #[test]
    fn restore_rows_show_the_window_title_and_other_rows_their_label() {
        let items = [Item::Restore(key_of(4)), Item::Close(key_of(4))];
        let titles = vec!["Report".to_string()];
        assert_eq!(row_label(&items, &titles, 0), "Report");
        // No title for the row (a fixed-label menu): the action's own label.
        assert_eq!(row_label(&items, &titles, 1), "Close window");
        assert_eq!(row_label(&items, &[], 0), "Window");
    }
}
