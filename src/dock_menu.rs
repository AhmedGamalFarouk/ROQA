//! Dock context menu (features B3): right-click on a slot opens a small owned popup above it with the
//! actions that apply to that slot. It is not `TrackPopupMenu`, which is modal and would stop the host
//! loop. The popup exists only while open, takes the foreground, and ends on a
//! press elsewhere, losing the foreground, its slot changing or a full-screen app; closed, it costs nothing.
use crate::{
    dock::{self, DockItem},
    edge::{ListRow, list_layout, mix_percent, row_rect},
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
        UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus},
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
    Battery,
    /// An AI limit window: its used percent, drawn as a capsule bar; its text is in `Spec::titles`.
    Limit(u8),
    /// The moon popup's hero: the lit share in percent and whether the lit limb is on the right; its three
    /// lines are in `Spec::titles`. It stands `MOON_HERO` DIPs taller than a row.
    Moon(u8, bool),
    /// A read-only two-column row (moon dates, weather hours, Command Center readouts): `Spec::titles` holds
    /// "label\tvalue".
    Pair,
    CalendarWeek(Option<u8>),
    Widget(WidgetAction),
    /// A slider row of the volume and brightness popups: its level in percent and whether it is muted; its name
    /// is in `Spec::titles`.
    Level(u8, bool),
    /// An output device row of the volume popup (an index into the host's list) and whether it is the default.
    Device(usize, bool),
    NewInstance,
    /// Select the app's executable in File Explorer.
    OpenFileLocation,
    Close(WindowKey),
    CloseAll,
    /// Keep the one window above the others, or let it go (which one is offered follows its state).
    OnTop(WindowKey),
    OffTop(WindowKey),
    Pin,
    Unpin,
    /// Hide a running, unpinned app from the dock (Settings lists it to bring back).
    Hide,
    RemoveShelf,
    UndoShelf,
    /// The app menu (right-click on the notch or the dock outside an app slot).
    Settings,
    Quit,
    /// A row of the quick settings popup; its text is the setting's state (`Spec::titles`).
    Quick(Quick),
    QuickTile(Quick, bool, bool),
    WindowsLink(WindowsLink),
    /// A row of the Places popup (an index into the host's list); its text is in `Spec::titles`.
    Place(usize),
    /// The Eject row under a USB drive's Place row (the same index); its text is in `Spec::titles`.
    Eject(usize),
    /// A row of a folder stack (an index into the host's list of files); its text is in `Spec::titles`.
    File(usize),
    /// Pin the Downloads folder to the dock (offered in the app menu until it is pinned).
    PinDownloads,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetAction {
    MonthBack,
    MonthNext,
    WaterAdd,
    WaterRemove,
    TimerPause,
    TimerExtend,
    TimerResume,
    TimerCancel,
}

/// What a quick settings row does when pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quick {
    Mute,
    Airplane,
    Hotspot,
    WifiPage,
    BluetoothPage,
    WifiSettings,
    BluetoothSettings,
    Connect(usize),
    Back,
    WiFi,
    Bluetooth,
    Power,
    KeepAwake,
    Lock,
    Sleep,
    Theme,
}
/// Fixed, ordinary shell launches from the dock Windows button's right-click menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsLink {
    Settings,
    Apps,
    System,
    Network,
    Power,
    Explorer,
    TaskManager,
}
impl WindowsLink {
    pub const ALL: [Self; 7] = [
        Self::Settings,
        Self::Apps,
        Self::System,
        Self::Network,
        Self::Power,
        Self::Explorer,
        Self::TaskManager,
    ];
    pub fn details(self) -> (&'static str, &'static str) {
        match self {
            Self::Settings => ("Windows settings", "ms-settings:"),
            Self::Apps => ("Installed apps", "ms-settings:appsfeatures"),
            Self::System => ("System", "ms-settings:about"),
            Self::Network => ("Network settings", "ms-settings:network"),
            Self::Power => ("Power and battery", "ms-settings:powersleep"),
            Self::Explorer => ("File Explorer", "explorer.exe"),
            Self::TaskManager => ("Task Manager", "taskmgr.exe"),
        }
    }
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
            Item::Label
            | Item::Battery
            | Item::Limit(_)
            | Item::Moon(..)
            | Item::Pair
            | Item::CalendarWeek(_)
            | Item::Widget(_)
            | Item::Level(..)
            | Item::Device(..) => "",
            Item::NewInstance => "New instance",
            Item::OpenFileLocation => "Open file location",
            Item::Close(_) => "Close window",
            Item::CloseAll => "Close all windows",
            Item::OnTop(_) => "Keep on top",
            Item::OffTop(_) => "Stop keeping on top",
            Item::Pin => "Pin to dock",
            Item::Unpin => "Unpin from dock",
            Item::Hide => "Hide from dock",
            Item::RemoveShelf => "Remove from Shelf",
            Item::UndoShelf => "Undo removal",
            Item::Settings => "Settings",
            Item::Quit => "Quit ROQA",
            Item::Quick(_) | Item::QuickTile(..) => "Setting",
            Item::WindowsLink(link) => link.details().0,
            Item::Place(_) => "Place",
            Item::Eject(_) => "Eject",
            Item::File(_) => "File",
            Item::PinDownloads => "Pin Downloads",
        }
    }
}

/// The `.exe` that Open file location selects: an absolute drive-letter path only, so a packaged app, a relative
/// path and a UNC path never qualify. Whether it is still a local file is checked on activation (`shelf_os::reveal_local`).
pub fn file_location(identity: &Identity) -> Option<String> {
    let path = pins::launch_target(identity)?;
    let bytes = path.as_bytes();
    (bytes.get(1) == Some(&b':')
        && bytes.get(2) == Some(&b'\\')
        && path.to_lowercase().ends_with(".exe"))
    .then_some(path)
}

/// The actions that apply to a dock slot: New instance when the app can be launched, Close window only
/// while it has windows (a pinned app that is not running has none), and Pin or Unpin.
pub fn items(item: &DockItem, pins: &Pins) -> Vec<Item> {
    if !item.members.is_empty() {
        return vec![Item::OpenFolder, Item::Rename, Item::Dissolve];
    }
    let launchable = pins::launch_target(&item.identity).is_some();
    let pin = dock::pinned_as(item, pins);
    let mut list = Vec::with_capacity(4);
    if launchable {
        list.push(Item::NewInstance);
        // Not for a folder member: its menu keeps the folder actions only.
        if file_location(&item.identity).is_some()
            && pin.as_ref().is_none_or(|pin| pins.folder(pin).is_none())
        {
            list.push(Item::OpenFileLocation);
        }
    }
    match item.windows.as_slice() {
        [] => {}
        [key] => {
            list.push(Item::Close(*key));
            list.push(if dock::is_on_top(*key) {
                Item::OffTop(*key)
            } else {
                Item::OnTop(*key)
            });
        }
        _ => list.push(Item::CloseAll),
    }
    if let Some(pin) = pin {
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
    /// A slider row (by index) was dragged or wheeled to this percent.
    Level(usize, u8),
    /// A press on an app row's icon: mute it or unmute it.
    Mute(usize),
}

/// `Spec::slot` of a menu that describes no dock item (the app menu, quick settings, places, a folder stack).
pub const APP: usize = usize::MAX;

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
    /// Row text and icon per item when the labels are not fixed (quick settings, Places, folder rows); empty otherwise.
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
    /// The slider row the pointer holds, with the capture.
    grab: Option<usize>,
    work: Bounds,
    anchor: (i32, Bounds, Option<i32>),
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

/// DIPs the moon hero (92 high, then its divider and gap, 6) stands over one row.
const MOON_HERO: i32 = 98 - 34;
/// The extra height of a list that opens with the moon hero.
fn hero(items: &[Item], dpi: u32) -> i32 {
    if matches!(items.first(), Some(Item::Moon(..))) {
        dock::px(MOON_HERO, dpi)
    } else {
        0
    }
}

fn visible_rows(dpi: u32, work: Bounds, count: usize) -> usize {
    count.min(((work.height - dock::px(48, dpi)) / dock::px(34, dpi)).max(1) as usize)
}

fn popup_layout(dpi: u32, visible: usize, items: &[Item]) -> ((i32, i32), Bounds, usize) {
    let (mut size, mut panel) = list_layout(dpi, visible);
    let quick = matches!(
        items.first(),
        Some(Item::QuickTile(..) | Item::Quick(Quick::Back))
    );
    let grid = if visible == items.len() {
        items
            .iter()
            .take_while(|item| matches!(item, Item::QuickTile(..)))
            .count()
    } else {
        0
    };
    if quick {
        let width = dock::px(308, dpi);
        size.0 += width - panel.width;
        panel.width = width;
    }
    let extra = hero(items, dpi) + grid.div_ceil(2) as i32 * dock::px(52, dpi)
        - grid as i32 * dock::px(34, dpi);
    size.1 += extra;
    panel.height += extra;
    (size, panel, grid)
}

fn popup_row(dpi: u32, panel: Bounds, index: usize, grid: usize, hero: i32) -> Bounds {
    if index < grid {
        let gap = dock::px(4, dpi);
        let inset = gap * 2;
        let width = (panel.width - 2 * inset - gap) / 2;
        return Bounds {
            x: panel.x + inset + (index % 2) as i32 * (width + gap),
            y: panel.y + dock::px(6, dpi) + (index / 2) as i32 * dock::px(52, dpi),
            width,
            height: dock::px(48, dpi),
        };
    }
    let mut row = row_rect(dpi, panel, index - grid);
    row.y += grid.div_ceil(2) as i32 * dock::px(52, dpi);
    if index == 0 {
        row.height += hero;
    } else {
        row.y += hero;
    }
    row
}

impl Menu {
    /// Creates, paints and shows the popup (not yet the foreground: the host asks for that).
    pub fn open(graphics: &Graphics, palette: Palette, spec: Spec) -> Result<Self> {
        crate::places::invalidate(); // a folder stack still being read is not wanted any more
        Self::create(graphics, palette, spec, false)
    }
    pub fn open_hover(graphics: &Graphics, palette: Palette, spec: Spec) -> Result<Self> {
        Self::create(graphics, palette, spec, true)
    }
    fn create(graphics: &Graphics, palette: Palette, spec: Spec, hover: bool) -> Result<Self> {
        let dpi = spec.dpi;
        let visible_rows = visible_rows(dpi, spec.work, spec.items.len());
        let (mut size, _, _) = popup_layout(dpi, visible_rows, &spec.items);
        if hover && spec.items == [Item::Label] && !spec.app.is_empty() {
            size.0 = crate::graphics::dock_label_width(&spec.app, dpi)? + dock::px(6, dpi);
        }
        let slot_x = spec.x;
        let mut bounds = match spec.below {
            Some(bottom) => place_below(slot_x, bottom, spec.work, size),
            None => place(slot_x, spec.dock_bounds, spec.work, size),
        };
        if hover && spec.below.is_none() && spec.items == [Item::Label] && !spec.app.is_empty() {
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
            grab: None,
            work: spec.work,
            anchor: (spec.x, spec.dock_bounds, spec.below),
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
    /// A slider is being dragged: the host leaves the rows alone until it ends.
    pub fn held(&self) -> bool {
        self.grab.is_some()
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
    /// Icons that arrived after the menu opened: repaints at once.
    pub fn set_icons(&mut self, icons: Vec<Option<Arc<IconPixels>>>) -> Result<()> {
        self.icons = icons;
        self.drawn = None;
        self.paint()
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
        let (size, _, _) = popup_layout(self.dpi, self.visible_rows, &self.items);
        let (x, dock, below) = self.anchor;
        let bounds = match below {
            Some(bottom) => place_below(x, bottom, self.work, size),
            None => place(x, dock, self.work, size),
        };
        if self.bounds != bounds {
            self.bounds = bounds;
            self.window.resize(bounds)?;
            self.canvas.resize(bounds)?;
        }
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
        unsafe {
            self.grab.is_some()
                || GetCursorPos(&mut point).is_ok() && WindowFromPoint(point) == self.window.0
        }
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
    pub fn update_rows(&mut self, items: Vec<Item>, titles: Vec<String>) -> Result<()> {
        if self.items == items && self.titles == titles {
            return Ok(());
        }
        if self.items.len() == items.len()
            && self
                .items
                .iter()
                .zip(&items)
                .all(|(old, new)| std::mem::discriminant(old) == std::mem::discriminant(new))
        {
            if !self.held() {
                self.items = items;
                self.titles = titles;
                self.drawn = None;
            }
            return Ok(());
        }
        self.relist(items, titles, Vec::new())
    }

    pub fn paint(&mut self) -> Result<()> {
        let now = Instant::now();
        if self.drawn == Some(self.selected) && !self.reveal.running(now) {
            return Ok(());
        }
        let (_, mut panel, grid) = popup_layout(self.dpi, self.visible_rows, &self.items);
        panel.width = self.bounds.width - 2 * panel.x;
        let hero = hero(&self.items, self.dpi);
        let rows: Vec<ListRow> = self
            .items
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(self.visible_rows)
            .map(|(index, _)| ListRow {
                name: match (
                    self.items[index],
                    row_label(&self.items, &self.titles, index),
                ) {
                    (Item::Pair, text) => text.split_once('\t').map_or(text, |(label, _)| label),
                    (_, text) => text,
                },
                quick: match self.items[index] {
                    Item::QuickTile(action, on, details) => Some((action, on, details)),
                    _ => None,
                },
                calendar: match self.items[index] {
                    Item::CalendarWeek(today) => Some(today),
                    _ => None,
                },
                current: matches!(self.items[index], Item::Device(_, true)),
                icon: self.icons.get(index).and_then(|icon| icon.as_deref()),
                room: matches!(self.items[index], Item::File(_)),
                level: match self.items[index] {
                    Item::Level(pct, muted) => Some((pct, muted)),
                    _ => None,
                },
                bar: match self.items[index] {
                    Item::Limit(pct) => Some(pct),
                    _ => None,
                },
                moon: match self.items[index] {
                    Item::Moon(pct, right) => Some((f32::from(pct) / 100.0, right)),
                    _ => None,
                },
                value: (self.items[index] == Item::Pair)
                    .then(|| {
                        self.titles
                            .get(index)?
                            .split_once('\t')
                            .map(|(_, date)| date)
                    })
                    .flatten(),
                rect: popup_row(self.dpi, panel, index - self.offset, grid, hero),
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
        let (_, mut panel, grid) = popup_layout(self.dpi, self.visible_rows, &self.items);
        panel.width = self.bounds.width - 2 * panel.x;
        let extra = hero(&self.items, self.dpi);
        let at = |message: &MSG| {
            let (x, y) = point(message.lParam);
            (0..self.visible_rows)
                .find(|&index| {
                    let row = popup_row(self.dpi, panel, index, grid, extra);
                    (row.x..row.x + row.width).contains(&x)
                        && (row.y..row.y + row.height).contains(&y)
                })
                .map(|index| index + self.offset)
        };
        let is_level =
            |row: Option<usize>| row.is_some_and(|row| matches!(self.items[row], Item::Level(..)));
        let level = |menu: &mut Self, row: usize, x: i32| {
            let pct = mix_percent(
                x,
                popup_row(menu.dpi, panel, row - menu.offset, grid, extra),
            );
            menu.items[row] = Item::Level(pct, false);
            menu.drawn = None;
            Event::Level(row, pct)
        };
        match message.message {
            WM_MOUSEMOVE if self.grab.is_some() => {
                if let Some(row) = self.grab.filter(|_| message.wParam.0 & 1 != 0) {
                    return level(self, row, point(message.lParam).0);
                }
                self.grab = None;
                // SAFETY: releases this thread's own capture; harmless if it is already gone.
                let _ = unsafe { ReleaseCapture() };
            }
            WM_LBUTTONDOWN if is_level(at(message)) => {
                let row = at(message).unwrap_or_default();
                // An app's icon mutes it; the rest of the row is its slider.
                if self.icons.get(row).is_some_and(Option::is_some)
                    && point(message.lParam).0 < dock::px(crate::edge::MIX_ICON, self.dpi)
                {
                    return Event::Mute(row);
                }
                self.grab = Some(row);
                // SAFETY: captures the pointer for this thread's own live window.
                unsafe { SetCapture(self.window.0) };
                return level(self, row, point(message.lParam).0);
            }
            WM_LBUTTONUP if self.grab.take().is_some() => {
                // SAFETY: releases this thread's own capture.
                let _ = unsafe { ReleaseCapture() };
            }
            WM_MOUSEWHEEL if is_level(self.selected) => {
                let row = self.selected.unwrap_or_default();
                let Item::Level(pct, muted) = self.items[row] else {
                    return Event::Consumed;
                };
                let up = (message.wParam.0 >> 16) as u16 as i16 > 0;
                let step = if message.wParam.0 & 4 != 0 { 1 } else { 3 };
                let pct = (i32::from(pct) + if up { step } else { -step }).clamp(0, 100) as u8;
                self.items[row] = Item::Level(pct, muted && pct == 0);
                self.drawn = None;
                return Event::Level(row, pct);
            }
            WM_MOUSEMOVE => {
                self.selected = at(message).filter(|&row| {
                    !matches!(
                        self.items[row],
                        Item::Label
                            | Item::Battery
                            | Item::Limit(_)
                            | Item::Moon(..)
                            | Item::Pair
                            | Item::CalendarWeek(_)
                    )
                });
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
                return at(message).map_or(Event::Consumed, |row| {
                    if let Item::QuickTile(action, _, details) = self.items[row] {
                        let rect = popup_row(self.dpi, panel, row - self.offset, grid, extra);
                        let action = if details
                            && point(message.lParam).0
                                >= rect.x + rect.width - dock::px(32, self.dpi)
                        {
                            match action {
                                Quick::WiFi => Quick::WifiPage,
                                Quick::Bluetooth => Quick::BluetoothPage,
                                _ => action,
                            }
                        } else {
                            action
                        };
                        return Event::Choose(Item::Quick(action));
                    }
                    if matches!(
                        self.items[row],
                        Item::Battery
                            | Item::Limit(_)
                            | Item::Moon(..)
                            | Item::Pair
                            | Item::CalendarWeek(_)
                            | Item::Level(..)
                    ) {
                        Event::Consumed
                    } else {
                        Event::Choose(self.items[row])
                    }
                });
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
            recent: false,
        }
    }
    fn pinned(identity: &Identity) -> Pins {
        let mut pins = Pins::default();
        pins.add(identity.clone()).unwrap();
        pins
    }

    #[test]
    fn windows_links_are_fixed_ordinary_shell_targets() {
        let rows: Vec<_> = WindowsLink::ALL
            .into_iter()
            .map(|link| (Item::WindowsLink(link).label(), link.details().1))
            .collect();
        assert_eq!(
            rows,
            [
                ("Windows settings", "ms-settings:"),
                ("Installed apps", "ms-settings:appsfeatures"),
                ("System", "ms-settings:about"),
                ("Network settings", "ms-settings:network"),
                ("Power and battery", "ms-settings:powersleep"),
                ("File Explorer", "explorer.exe"),
                ("Task Manager", "taskmgr.exe"),
            ]
        );
    }

    #[test]
    fn windows_links_stay_above_the_clicked_start_on_its_monitor() {
        for dpi in [96, 144, 192] {
            for rtl in [false, true] {
                for work in [
                    Bounds {
                        x: -1920,
                        y: 80,
                        width: 1920,
                        height: 1080,
                    },
                    Bounds {
                        x: 1920,
                        y: -200,
                        width: 1920,
                        height: 1080,
                    },
                ] {
                    let buttons = dock::Buttons {
                        rtl,
                        battery: true,
                        ..dock::Buttons::default()
                    };
                    let width = dock::width_px(buttons, 3, dpi);
                    for x in [
                        work.x,
                        work.x + (work.width - width) / 2,
                        work.x + work.width - width,
                    ] {
                        let dock = Bounds {
                            x,
                            y: work.y + work.height - dock::px(64, dpi),
                            width,
                            height: dock::px(64, dpi),
                        };
                        let (left, cell) =
                            dock::span(buttons, width, dpi, dock::Hit::Button(dock::Button::Start));
                        let anchor = dock.x + left + cell / 2;
                        let size = (dock::px(266, dpi), dock::px(300, dpi));
                        let popup = place(anchor, dock, work, size);
                        assert_eq!(popup.y + popup.height, dock.y);
                        assert!(popup.x >= work.x && popup.x + popup.width <= work.x + work.width);
                        assert!(popup.y >= work.y);
                        if (work.x + size.0 / 2..=work.x + work.width - size.0 / 2)
                            .contains(&anchor)
                        {
                            assert_eq!(popup.x + popup.width / 2, anchor);
                        }
                    }
                }
            }
        }
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
                Item::OpenFileLocation,
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
                Item::OpenFileLocation,
                Item::Close(key_of(1)),
                Item::OnTop(key_of(1)),
                Item::Pin,
                Item::Hide,
                Item::Group
            ]
        );
        // Running (2 windows) and pinned: Unpin replaces Pin.
        assert_eq!(
            items(&running2, &pinned(&app)),
            [
                Item::NewInstance,
                Item::OpenFileLocation,
                Item::CloseAll,
                Item::Unpin,
                Item::Group
            ]
        );
        // A pinned app that is not running has no window to close.
        let idle = dock_item(app.clone(), &[]);
        assert_eq!(
            items(&idle, &pinned(&app)),
            [
                Item::NewInstance,
                Item::OpenFileLocation,
                Item::Unpin,
                Item::Group
            ]
        );
        // A packaged app launches through its AUMID; one with no usable identity can only close.
        let packaged = dock_item(Identity::Application("Contoso.Mail!App".into()), &[3]);
        assert_eq!(
            items(&packaged, &none),
            [
                Item::NewInstance,
                Item::Close(key_of(3)),
                Item::OnTop(key_of(3)),
                Item::Pin,
                Item::Hide,
                Item::Group
            ]
        );
        let unknown = dock_item(Identity::Unavailable(9), &[9]);
        assert_eq!(
            items(&unknown, &none),
            [Item::Close(key_of(9)), Item::OnTop(key_of(9))]
        );
    }

    #[test]
    fn file_location_only_for_absolute_local_looking_exes() {
        let yes = |identity: Identity| file_location(&identity).is_some();
        assert!(yes(exe(r"C:\Tools\Edit.EXE")));
        assert!(!yes(exe(r"\\server\share\a.exe")));
        assert!(!yes(exe(r"tools\a.exe")));
        assert!(!yes(exe(r"c:\tools\readme.txt")));
        assert!(!yes(Identity::Application("Contoso.Mail!App".into())));
        assert!(!yes(Identity::Unavailable(4)));
        // A folder member keeps the folder actions only: no file location.
        let first = exe(r"c:\first.exe");
        let mut pins = Pins::default();
        pins.group(&first, &exe(r"c:\second.exe"));
        let member = dock_item(first, &[1]);
        assert!(!items(&member, &pins).contains(&Item::OpenFileLocation));
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
    fn command_tiles_share_the_grid_without_overlapping_the_footer() {
        let items = [
            Item::QuickTile(Quick::WiFi, true, true),
            Item::QuickTile(Quick::Bluetooth, false, false),
            Item::QuickTile(Quick::Airplane, false, false),
            Item::Quick(Quick::Sleep),
        ];
        for dpi in [96, 144, 192] {
            let (_, panel, grid) = popup_layout(dpi, items.len(), &items);
            assert_eq!(grid, 3);
            let [left, right, next, footer] =
                std::array::from_fn(|index| popup_row(dpi, panel, index, grid, 0));
            assert!(left.x + left.width < right.x);
            assert_eq!(left.y, right.y);
            assert!(left.y + left.height < next.y);
            assert!(next.y + next.height <= footer.y);
            assert!(footer.y + footer.height <= panel.y + panel.height);
        }
    }

    #[test]
    fn rows_show_their_title_and_other_rows_their_label() {
        let items = [Item::File(0), Item::Close(key_of(4))];
        let titles = vec!["Report".to_string()];
        assert_eq!(row_label(&items, &titles, 0), "Report");
        // No title for the row (a fixed-label menu): the action's own label.
        assert_eq!(row_label(&items, &titles, 1), "Close window");
        assert_eq!(row_label(&items, &[], 0), "File");
    }
}
