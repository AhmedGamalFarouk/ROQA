//! Dock context menu (features B3): right-click on a slot opens a small owned popup above it with the
//! actions that apply to that slot. It is not `TrackPopupMenu`, which is modal and would stop the host
//! loop. The popup exists only while open, takes the foreground for the keyboard, and ends on Escape, a
//! press elsewhere, losing the foreground, its slot changing or a full-screen app; closed, it costs nothing.
use crate::{
    dock::{self, DockItem},
    dock_uia::Accessibility,
    edge::{ListRow, list_layout, row_at, row_rect},
    graphics::{Canvas, Graphics, ListScene},
    layout::Bounds,
    menu_uia,
    native_window::{self, NativeWindow},
    pins::{self, Pins},
    theme::Palette,
    window_model::{Identity, WindowKey},
};
use std::sync::{Arc, Mutex};
use windows::{
    Win32::{
        Foundation::HWND,
        UI::{
            Input::KeyboardAndMouse::{
                VIRTUAL_KEY, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_SPACE, VK_TAB, VK_UP,
            },
            WindowsAndMessaging::{
                MSG, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MOUSEMOVE,
                WM_RBUTTONDOWN, WM_RBUTTONUP,
            },
        },
    },
    core::Result,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    NewInstance,
    Close,
    Pin,
    Unpin,
}
impl Item {
    pub fn label(self) -> &'static str {
        match self {
            Item::NewInstance => "New instance",
            Item::Close => "Close window",
            Item::Pin => "Pin to dock",
            Item::Unpin => "Unpin from dock",
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
    if !item.windows.is_empty() {
        list.push(Item::Close);
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
pub fn describes(identity: &Identity, shown: &[Item], now: Option<&DockItem>, pins: &Pins) -> bool {
    now.is_some_and(|now| now.identity == *identity && items(now, pins) == shown)
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

/// What a key does to the menu.
#[derive(Debug, PartialEq, Eq)]
pub enum Key {
    Select(usize),
    Choose(usize),
    Dismiss,
    Ignore,
}

/// Down and Tab step forward and Up back, wrapping; Enter and Space choose the highlighted row (once per
/// press, not on auto-repeat); Escape dismisses.
pub fn key(vk: VIRTUAL_KEY, selected: Option<usize>, len: usize, repeat: bool) -> Key {
    match vk {
        _ if len == 0 => Key::Ignore,
        VK_ESCAPE => Key::Dismiss,
        VK_DOWN | VK_TAB => Key::Select(selected.map_or(0, |at| (at + 1) % len)),
        VK_UP => Key::Select(selected.map_or(len - 1, |at| (at + len - 1) % len)),
        VK_RETURN | VK_SPACE => match selected {
            Some(at) if !repeat => Key::Choose(at),
            _ => Key::Ignore,
        },
        _ => Key::Ignore,
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
    pub items: Vec<Item>,
    pub dock_bounds: Bounds,
    pub work: Bounds,
    pub dpi: u32,
    pub app: String,
    /// The window that had the foreground, which gets it back when the menu ends.
    pub return_to: Option<WindowKey>,
}

pub struct Menu {
    // UIA is released first, then the canvas, then the window.
    accessibility: Accessibility,
    canvas: Canvas,
    window: NativeWindow,
    uia: menu_uia::Shared,
    dock: HWND,
    pub slot: usize,
    pub identity: Identity,
    items: Vec<Item>,
    dpi: u32,
    selected: Option<usize>,
    drawn: Option<Option<usize>>,
    pub return_to: Option<WindowKey>,
}
impl Drop for Menu {
    fn drop(&mut self) {
        self.accessibility.disconnect();
        menu_uia::invalidate(&self.uia);
    }
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
        let (size, panel) = list_layout(dpi, spec.items.len());
        let slot_x = dock::slot_center_x(spec.dock_bounds.x, spec.slot, dpi);
        let bounds = place(slot_x, spec.dock_bounds, spec.work, size);
        let window = NativeWindow::create_menu(bounds, &format!("{} menu", spec.app))?;
        let uia: menu_uia::Shared = Arc::new(Mutex::new(menu_uia::State {
            name: format!("{} menu", spec.app),
            rows: spec
                .items
                .iter()
                .enumerate()
                .map(|(index, item)| (item.label().to_owned(), row_rect(dpi, panel, index)))
                .collect(),
            ..Default::default()
        }));
        let accessibility = menu_uia::accessibility(window.0, &uia)?;
        let surface = Bounds {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        let canvas = Canvas::create(graphics, window.0, surface, palette)?;
        let mut menu = Self {
            accessibility,
            canvas,
            window,
            uia,
            dock: spec.dock,
            slot: spec.slot,
            identity: spec.identity,
            items: spec.items,
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
            .map(|(index, item)| ListRow {
                name: item.label(),
                current: false,
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
    /// One message: mouse and key input for the menu's window, UIA invokes, losing the foreground, and a
    /// button press on any other ROQA window.
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
            menu_uia::INVOKE => {
                return self
                    .items
                    .get(message.wParam.0)
                    .map_or(Event::Consumed, |&item| Event::Choose(item));
            }
            WM_KEYDOWN => {
                let repeat = message.lParam.0 & (1 << 30) != 0;
                let vk = VIRTUAL_KEY(message.wParam.0 as u16);
                match key(vk, self.selected, self.items.len(), repeat) {
                    Key::Select(row) => self.selected = Some(row),
                    Key::Choose(row) => return Event::Choose(self.items[row]),
                    Key::Dismiss => return Event::Dismiss,
                    Key::Ignore => {}
                }
            }
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
        // Running, unpinned: all three; close is offered because it has windows.
        let running = dock_item(app.clone(), &[1, 2]);
        assert_eq!(
            items(&running, &none),
            [Item::NewInstance, Item::Close, Item::Pin]
        );
        // Running and pinned: Unpin replaces Pin.
        assert_eq!(
            items(&running, &pinned(&app)),
            [Item::NewInstance, Item::Close, Item::Unpin]
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
            [Item::NewInstance, Item::Close, Item::Pin]
        );
        let unknown = dock_item(Identity::Unavailable(9), &[9]);
        assert_eq!(items(&unknown, &none), [Item::Close]);
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
    fn the_menu_ends_when_its_slot_no_longer_matches() {
        let app = exe(r"c:\app.exe");
        let pins = Pins::default();
        let shown = items(&dock_item(app.clone(), &[1]), &pins);
        let same = dock_item(app.clone(), &[1, 7]);
        // More windows in the same app change nothing; the slot gone, another app or other actions do.
        assert!(describes(&app, &shown, Some(&same), &pins));
        assert!(!describes(&app, &shown, None, &pins));
        let other = dock_item(exe(r"c:\other.exe"), &[1]);
        assert!(!describes(&app, &shown, Some(&other), &pins));
        let closed = dock_item(app.clone(), &[]);
        assert!(!describes(&app, &shown, Some(&closed), &pins));
        assert!(!describes(&app, &shown, Some(&same), &pinned(&app)));
    }

    #[test]
    fn arrows_wrap_enter_chooses_once_and_escape_dismisses() {
        let at = |vk, selected, repeat| key(vk, selected, 3, repeat);
        assert_eq!(at(VK_DOWN, None, false), Key::Select(0));
        assert_eq!(at(VK_DOWN, Some(2), false), Key::Select(0));
        assert_eq!(at(VK_TAB, Some(0), false), Key::Select(1));
        assert_eq!(at(VK_UP, None, false), Key::Select(2));
        assert_eq!(at(VK_UP, Some(0), false), Key::Select(2));
        assert_eq!(at(VK_RETURN, Some(1), false), Key::Choose(1));
        assert_eq!(at(VK_SPACE, Some(1), false), Key::Choose(1));
        // Nothing highlighted, or a held key, chooses nothing.
        assert_eq!(at(VK_RETURN, None, false), Key::Ignore);
        assert_eq!(at(VK_RETURN, Some(1), true), Key::Ignore);
        assert_eq!(at(VK_ESCAPE, Some(1), false), Key::Dismiss);
        assert_eq!(at(VIRTUAL_KEY(0x41), Some(1), false), Key::Ignore);
        assert_eq!(key(VK_DOWN, None, 0, false), Key::Ignore);
    }
}
