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
    motion::{self, Tween},
    native_window::{self, NativeWindow},
    picker_font::PickerFont,
    pins::{self, Pins},
    theme::Palette,
    window_model::{Identity, WindowKey},
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::{COLORREF, HANDLE, HWND, LPARAM, POINT},
        Graphics::Gdi::{CreateSolidBrush, DeleteObject, HBRUSH},
        UI::Controls::EM_SETLIMITTEXT,
        UI::Input::KeyboardAndMouse::SetFocus,
        UI::WindowsAndMessaging::{
            CreateWindowExW, ES_AUTOHSCROLL, GetCursorPos, GetWindowTextW, MSG, RemovePropW,
            SendMessageW, SetPropW, WINDOW_STYLE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
            WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETFONT, WS_CHILD,
            WS_VISIBLE, WindowFromPoint,
        },
    },
    core::{HSTRING, Result, w},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    OpenFolder,
    Group,
    GroupTarget(usize),
    Rename,
    SaveName,
    Dissolve,
    RemoveMember,
    Member(usize),
    MemberMenu(usize),
    Label,
    NewInstance,
    Close(WindowKey),
    CloseAll,
    Pin,
    Unpin,
    /// Hide a running, unpinned app from the dock (Settings lists it to bring back).
    Hide,
    RemoveShelf,
    UndoShelf,
    /// The app menu (right-click on the notch or the dock outside an app slot).
    Settings,
    Quit,
    /// A minimized window in the chevron's list; its row shows the window's title (`Spec::titles`).
    Restore(WindowKey),
    /// A row of the quick settings popup; its text is the setting's state (`Spec::titles`).
    Quick(Quick),
    /// A row of the Places popup (an index into the host's list); its text is in `Spec::titles`.
    Place(usize),
}
/// What a quick settings row does when pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quick {
    Mute,
    VolumeUp,
    VolumeDown,
    WiFi,
    Bluetooth,
    Power,
    KeepAwake,
}
impl Item {
    pub fn label(self) -> &'static str {
        match self {
            Item::OpenFolder => "Open together",
            Item::Group => "Add to folder",
            Item::GroupTarget(_) => "New folder",
            Item::Rename => "Rename",
            Item::SaveName => "Save name",
            Item::Dissolve => "Ungroup apps",
            Item::RemoveMember => "Remove from folder",
            Item::Member(_) | Item::MemberMenu(_) => "App",
            Item::Label => "",
            Item::NewInstance => "New instance",
            Item::Close(_) => "Close window",
            Item::CloseAll => "Close all windows",
            Item::Pin => "Pin to dock",
            Item::Unpin => "Unpin from dock",
            Item::Hide => "Hide from dock",
            Item::RemoveShelf => "Remove from Shelf",
            Item::UndoShelf => "Undo removal",
            Item::Settings => "Settings",
            Item::Quit => "Quit ROQA",
            Item::Restore(_) => "Window",
            Item::Quick(_) => "Setting",
            Item::Place(_) => "Place",
        }
    }
}

/// The actions that apply to a dock slot: New instance when the app can be launched, Close window only
/// while it has windows (a pinned app that is not running has none), and Pin or Unpin.
pub fn items(item: &DockItem, pins: &Pins) -> Vec<Item> {
    if !item.members.is_empty() {
        return vec![Item::OpenFolder, Item::Rename, Item::Dissolve];
    }
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
    if let Some(pin) = dock::pinned_as(item, pins) {
        list.push(Item::Unpin);
        if pins.folder(&pin).is_some() {
            list.push(Item::RemoveMember);
        }
    } else if launchable {
        list.push(Item::Pin);
        list.push(Item::Hide);
    }
    if launchable {
        list.push(Item::Group);
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
    now.and_then(|tile| {
        if tile.identity == *identity
            && (tile.members.is_empty() || !shown.contains(&Item::RemoveMember))
        {
            Some(tile)
        } else {
            tile.members
                .iter()
                .find(|member| member.identity == *identity)
        }
    })
    .is_some_and(|now| {
        now.identity == *identity
            && (shown.contains(&Item::SaveName)
                || shown
                    .iter()
                    .any(|item| matches!(item, Item::GroupTarget(_)))
                || (now.windows == windows && shown == items(now, pins)))
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
    pub targets: Vec<Identity>,
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
    pub bounds: Bounds,
    pub hover_popup: bool,
    pub hover_hit: Option<dock::Hit>,
    pub close_at: Option<Instant>,
    reveal: Tween,
    visible_rows: usize,
    offset: usize,
    edit: Option<HWND>,
    work: Bounds,
    palette: Palette,
    edit_brush: Option<HBRUSH>,
    edit_font: Option<PickerFont>,
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

fn visible_rows(dpi: u32, work: Bounds, count: usize) -> usize {
    count.min(((work.height - dock::px(48, dpi)) / dock::px(34, dpi)).max(1) as usize)
}

impl Menu {
    /// Creates, paints and shows the popup (not yet the foreground: the host asks for that).
    pub fn open(graphics: &Graphics, palette: Palette, spec: Spec) -> Result<Self> {
        Self::create(graphics, palette, spec, false)
    }
    pub fn open_hover(graphics: &Graphics, palette: Palette, spec: Spec) -> Result<Self> {
        Self::create(graphics, palette, spec, true)
    }
    fn create(graphics: &Graphics, palette: Palette, spec: Spec, hover: bool) -> Result<Self> {
        let dpi = spec.dpi;
        let visible_rows = visible_rows(dpi, spec.work, spec.items.len());
        let (mut size, _) = list_layout(dpi, visible_rows);
        if hover && spec.items == [Item::Label] {
            size.0 = crate::graphics::dock_label_width(&spec.app, dpi)? + dock::px(6, dpi);
        }
        let slot_x = spec.x;
        let mut bounds = match spec.below {
            Some(bottom) => place_below(slot_x, bottom, spec.work, size),
            None => place(slot_x, spec.dock_bounds, spec.work, size),
        };
        if hover && spec.below.is_none() && spec.items == [Item::Label] {
            // The hover label sits close over its icon, not a whole menu gap above the bar.
            bounds.y += dock::px(6, dpi);
        }
        let window = NativeWindow::create_menu(bounds, &format!("{} menu", spec.app))?;
        let surface = Bounds {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        let canvas = Canvas::create(graphics, window.0, surface, palette)?;
        let mut menu = Self {
            targets: Vec::new(),
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
            bounds,
            hover_popup: false,
            hover_hit: None,
            close_at: None,
            reveal: Tween::rest(1.0),
            visible_rows,
            offset: 0,
            edit: None,
            work: spec.work,
            palette,
            edit_brush: None,
            edit_font: None,
        };
        if hover {
            menu.hover();
        }
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
    pub fn hover(&mut self) {
        self.hover_popup = true;
        self.reveal = Tween::rest(0.0);
        self.reveal.retarget(
            1.0,
            if motion::calm() {
                Duration::ZERO
            } else {
                motion::POP_TIME
            },
            motion::POP,
        );
        self.drawn = None;
    }
    pub fn relist(
        &mut self,
        items: Vec<Item>,
        titles: Vec<String>,
        icons: Vec<Option<Arc<IconPixels>>>,
    ) -> Result<()> {
        self.items = items;
        self.titles = titles;
        self.icons = icons;
        self.visible_rows = visible_rows(self.dpi, self.work, self.items.len());
        let (size, _) = list_layout(self.dpi, self.visible_rows);
        self.bounds = place(
            self.bounds.x + self.bounds.width / 2,
            Bounds {
                y: self.bounds.y + self.bounds.height,
                ..self.bounds
            },
            self.work,
            size,
        );
        self.window.resize(self.bounds)?;
        self.canvas.resize(self.bounds)?;
        self.offset = 0;
        self.selected = None;
        self.drawn = None;
        self.paint()
    }
    pub fn running(&self, now: Instant) -> bool {
        self.reveal.running(now)
    }
    pub fn closing(&self) -> bool {
        self.reveal.target() == 0.0
    }
    pub fn set_open(&mut self, open: bool) {
        let (target, duration, curve) = if open {
            (1.0, motion::POP_TIME, motion::POP)
        } else {
            (0.0, motion::HOVER_OUT_TIME, motion::SETTLE)
        };
        self.reveal.retarget(
            target,
            if motion::calm() {
                Duration::ZERO
            } else {
                duration
            },
            curve,
        );
        self.drawn = None;
    }
    pub fn under_pointer(&self) -> bool {
        let mut point = POINT::default();
        // SAFETY: only scalar cursor coordinates and HWND identity are read.
        unsafe { GetCursorPos(&mut point).is_ok() && WindowFromPoint(point) == self.window.0 }
    }
    pub fn rename(&mut self, name: &str) -> Result<()> {
        self.relist(vec![Item::Label, Item::SaveName], Vec::new(), Vec::new())?;
        let (_, panel) = list_layout(self.dpi, 2);
        let row = row_rect(self.dpi, panel, 0);
        let text = HSTRING::from(name);
        self.edit_font = Some(PickerFont::create(self.dpi, 1.0)?);
        // SAFETY: the edit is a standard child of this live owned window; Windows copies its title.
        unsafe {
            let rgb = |color: (u8, u8, u8)| {
                u32::from(color.0) | (u32::from(color.1) << 8) | (u32::from(color.2) << 16)
            };
            let background = rgb((
                self.palette.body.0,
                self.palette.body.1,
                self.palette.body.2,
            ));
            let brush = CreateSolidBrush(COLORREF(background));
            self.edit_brush = Some(brush);
            SetPropW(self.window.0, w!("ROQA.EditBrush"), Some(HANDLE(brush.0)))?;
            SetPropW(
                self.window.0,
                w!("ROQA.EditInk"),
                Some(HANDLE(rgb(self.palette.ink) as usize as *mut _)),
            )?;
            SetPropW(
                self.window.0,
                w!("ROQA.EditFill"),
                Some(HANDLE(background as usize as *mut _)),
            )?;
            let edit = CreateWindowExW(
                Default::default(),
                w!("EDIT"),
                &text,
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                row.x + dock::px(10, self.dpi),
                row.y + dock::px(6, self.dpi),
                row.width - dock::px(20, self.dpi),
                row.height - dock::px(10, self.dpi),
                Some(self.window.0),
                None,
                None,
                None,
            )?;
            if let Some(font) = &self.edit_font {
                SendMessageW(
                    edit,
                    WM_SETFONT,
                    Some(windows::Win32::Foundation::WPARAM(font.handle.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
            SendMessageW(
                edit,
                EM_SETLIMITTEXT,
                Some(windows::Win32::Foundation::WPARAM(40)),
                Some(LPARAM(0)),
            );
            let _ = SetFocus(Some(edit));
            self.edit = Some(edit);
        }
        Ok(())
    }
    pub fn edited_name(&self) -> String {
        let mut text = [0u16; 81];
        let length = self.edit.map_or(0, |edit| {
            // SAFETY: this owned child outlives the call and the output buffer is bounded.
            unsafe { GetWindowTextW(edit, &mut text) }
        }) as usize;
        String::from_utf16_lossy(&text[..length])
    }
    /// Repaints only when the highlighted row changed.
    pub fn paint(&mut self) -> Result<()> {
        let now = Instant::now();
        if self.drawn == Some(self.selected) && !self.reveal.running(now) {
            return Ok(());
        }
        let (_, mut panel) = list_layout(self.dpi, self.visible_rows);
        panel.width = self.bounds.width - 2 * panel.x;
        let rows: Vec<ListRow> = self
            .items
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(self.visible_rows)
            .map(|(index, _)| ListRow {
                name: row_label(&self.items, &self.titles, index),
                current: false,
                icon: self.icons.get(index).and_then(|icon| icon.as_deref()),
                level: None,
                rect: row_rect(self.dpi, panel, index - self.offset),
            })
            .collect();
        self.canvas.paint_list(&ListScene {
            reveal: self.reveal.value(now),
            dpi: self.dpi,
            panel,
            rows: &rows,
            hover: self.selected.map(|selected| selected - self.offset),
        })?;
        self.drawn = if self.reveal.running(now) {
            None
        } else {
            Some(self.selected)
        };
        Ok(())
    }
    /// One message: mouse input for the menu's window, losing the foreground, and a button press on any
    /// other ROQA window.
    pub fn message(&mut self, message: &MSG) -> Event {
        if self.edit == Some(message.hwnd) {
            return Event::Ignored;
        }
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
            row_at(self.dpi, self.visible_rows, x, y).map(|row| row + self.offset)
        };
        match message.message {
            WM_MOUSEMOVE => {
                self.selected = at(message).filter(|&row| self.items[row] != Item::Label);
                if self.hover_popup {
                    use windows::Win32::UI::Input::KeyboardAndMouse::{
                        TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
                    };
                    // SAFETY: registers a single leave notification for this live popup.
                    let _ = unsafe {
                        TrackMouseEvent(&mut TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: self.window.0,
                            dwHoverTime: 0,
                        })
                    };
                }
            }
            windows::Win32::UI::Controls::WM_MOUSELEAVE => self.selected = None,
            WM_LBUTTONUP => {
                return at(message).map_or(Event::Consumed, |row| Event::Choose(self.items[row]));
            }
            WM_RBUTTONUP if self.hover_popup => {
                return at(message).map_or(Event::Consumed, |row| match self.items[row] {
                    Item::Member(index) => Event::Choose(Item::MemberMenu(index)),
                    _ => Event::Consumed,
                });
            }
            WM_MOUSEWHEEL => {
                let delta = (message.wParam.0 >> 16) as u16 as i16;
                self.offset = if delta > 0 {
                    self.offset.saturating_sub(3)
                } else {
                    (self.offset + 3).min(self.items.len() - self.visible_rows)
                };
                self.selected = None;
                self.drawn = None;
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_RBUTTONUP => {}
            native_window::DEACTIVATED if !self.hover_popup => return Event::Dismiss,
            _ => return Event::Ignored,
        }
        Event::Consumed
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        if let Some(brush) = self.edit_brush.take() {
            // SAFETY: these properties and this brush belong to this still-live parent window.
            unsafe {
                let _ = RemovePropW(self.window.0, w!("ROQA.EditBrush"));
                let _ = RemovePropW(self.window.0, w!("ROQA.EditInk"));
                let _ = RemovePropW(self.window.0, w!("ROQA.EditFill"));
                let _ = DeleteObject(brush.into());
            }
        }
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
            members: Vec::new(),
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
            [
                Item::NewInstance,
                Item::CloseAll,
                Item::Pin,
                Item::Hide,
                Item::Group
            ]
        );
        // Running (1 window), unpinned: Close is offered.
        let running1 = dock_item(app.clone(), &[1]);
        assert_eq!(
            items(&running1, &none),
            [
                Item::NewInstance,
                Item::Close(key_of(1)),
                Item::Pin,
                Item::Hide,
                Item::Group
            ]
        );
        // Running (2 windows) and pinned: Unpin replaces Pin.
        assert_eq!(
            items(&running2, &pinned(&app)),
            [Item::NewInstance, Item::CloseAll, Item::Unpin, Item::Group]
        );
        // A pinned app that is not running has no window to close.
        let idle = dock_item(app.clone(), &[]);
        assert_eq!(
            items(&idle, &pinned(&app)),
            [Item::NewInstance, Item::Unpin, Item::Group]
        );
        // A packaged app launches through its AUMID; one with no usable identity can only close.
        let packaged = dock_item(Identity::Application("Contoso.Mail!App".into()), &[3]);
        assert_eq!(
            items(&packaged, &none),
            [
                Item::NewInstance,
                Item::Close(key_of(3)),
                Item::Pin,
                Item::Hide,
                Item::Group
            ]
        );
        let unknown = dock_item(Identity::Unavailable(9), &[9]);
        assert_eq!(items(&unknown, &none), [Item::Close(key_of(9))]);
    }

    #[test]
    fn folder_and_member_menus_keep_their_own_targets() {
        let first = exe(r"c:\first.exe");
        let second = exe(r"c:\second.exe");
        let mut pins = Pins::default();
        pins.group(&first, &second);
        let member = dock_item(first.clone(), &[1]);
        let mut folder = member.clone();
        folder.members = vec![member.clone(), dock_item(second, &[])];
        assert_eq!(
            items(&folder, &pins),
            [Item::OpenFolder, Item::Rename, Item::Dissolve]
        );
        let shown = items(&member, &pins);
        assert!(shown.contains(&Item::RemoveMember));
        assert!(describes(
            &first,
            &shown,
            &member.windows,
            Some(&folder),
            &pins
        ));
        assert!(describes(
            &first,
            &shown,
            &member.windows,
            Some(&member),
            &pins
        ));
        assert!(describes(
            &first,
            &[Item::Label, Item::SaveName],
            &[],
            Some(&folder),
            &pins
        ));
        assert!(!describes(
            &exe(r"c:\other.exe"),
            &shown,
            &member.windows,
            Some(&folder),
            &pins
        ));
    }

    #[test]
    fn large_folders_fit_the_work_area_at_each_scale() {
        for dpi in [96, 120, 144, 192] {
            let work = Bounds {
                x: -1920,
                y: -100,
                width: 1920,
                height: 1080,
            };
            let count = visible_rows(dpi, work, crate::pins::MAX_PINS + 1);
            let (size, _) = list_layout(dpi, count);
            assert!(count > 0 && count <= crate::pins::MAX_PINS + 1);
            assert!(size.1 <= work.height);
            let dock = Bounds {
                y: work.y + work.height - dock::px(64, dpi),
                ..work
            };
            let bounds = place(-100, dock, work, size);
            assert!(bounds.y >= work.y && bounds.y + bounds.height <= work.y + work.height);
        }
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
