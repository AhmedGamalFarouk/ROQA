use crate::pins::{self, Pins};
use crate::sort;
use crate::window_model::{Identity, WindowKey, WindowModel};
use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT, WPARAM},
    Globalization::{
        GetLocaleInfoW, GetUserDefaultUILanguage, LOCALE_IREADINGLAYOUT, LOCALE_RETURN_NUMBER,
    },
    System::Threading::GetCurrentProcessId,
    UI::{
        Accessibility::HWINEVENTHOOK,
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
            VK_B, VK_D, VK_LWIN, VK_RETURN, VK_S, VK_TAB, VK_X,
        },
        Shell::{SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW},
        WindowsAndMessaging::{
            EnumWindows, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW,
            GetWindowRect, GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST, IsIconic,
            IsWindow, IsWindowVisible, PostMessageW, SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow,
            SetWindowPos, ShowWindowAsync, WM_CLOSE, WS_EX_TOPMOST,
        },
    },
};
use windows::core::{BOOL, HSTRING, PCWSTR};

/// Dock geometry in DIPs. Slots are the app groups, in `items` order.
pub const SLOT_DIPS: i32 = 56;
pub const PAD_DIPS: i32 = 8;
pub const ICON_DIPS: i32 = 40;
pub const HEIGHT_DIPS: i32 = 64;
/// The bar's corner radius: a capsule, half the bar's height (the floating bar's, and the attached dock's top corners).
pub const RADIUS_DIPS: i32 = HEIGHT_DIPS / 2;

/// A title-derived indicator. Counts saturate at 100, displayed as 99+.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Badge {
    #[default]
    None,
    Dot,
    Count(u8),
}

/// Only edge tokens are interpreted, avoiding numbers embedded in ordinary titles.
pub fn title_badge(title: &str) -> Badge {
    let title = title.trim();
    let mut badge = Badge::None;
    if title.starts_with('?') || title.ends_with('?') {
        badge = Badge::Dot;
    }
    for (open, close) in [('(', ')'), ('[', ']')] {
        let token = if let Some(rest) = title.strip_prefix(open) {
            rest.split_once(close).map(|(value, _)| value)
        } else if let Some(rest) = title.strip_suffix(close) {
            rest.rsplit_once(open).map(|(_, value)| value)
        } else {
            None
        };
        if let Some(token) = token {
            let (digits, plus) = token
                .strip_suffix('+')
                .map_or((token, false), |v| (v, true));
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                let value = digits
                    .bytes()
                    .fold(0u8, |n, b| n.saturating_mul(10).saturating_add(b - b'0'))
                    .min(100);
                if value > 0 {
                    badge = badge.max(Badge::Count(if plus && value >= 99 { 100 } else { value }));
                }
            }
        }
    }
    badge
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockItem {
    pub identity: Identity,
    pub executable: Option<String>,
    pub windows: Vec<WindowKey>,
    pub active: bool,
    pub badge: Badge,
    pub members: Vec<DockItem>,
}

/// One item per real catalog group, in stable identity order. No placeholder rows. With `desktop_only`, a
/// window on another virtual desktop does not count (and a group with no other window disappears).
pub fn items(model: &WindowModel, desktop_only: bool) -> Vec<DockItem> {
    let mut items: Vec<DockItem> = Vec::new();
    let mut windows: Vec<_> = model
        .windows
        .iter()
        .filter(|window| window.current_desktop || !desktop_only)
        .collect();
    sort::by(&mut windows, |a, b| {
        (&a.identity, a.key).cmp(&(&b.identity, b.key))
    });
    for window in windows {
        match items.last_mut() {
            Some(item) if item.identity == window.identity => {
                item.windows.push(window.key);
                item.active |= window.foreground;
                item.badge = item.badge.max(title_badge(&window.title));
                if item.executable.is_none() {
                    item.executable.clone_from(&window.executable);
                }
            }
            _ => items.push(DockItem {
                identity: window.identity.clone(),
                executable: window.executable.clone(),
                windows: vec![window.key],
                active: window.foreground,
                badge: title_badge(&window.title),
                members: Vec::new(),
            }),
        }
    }
    items
}

/// Pins first, in pin order (running group or a launcher with no windows), then unpinned groups in `items` order.
/// An `Executable` pin also claims a group whose AUMID identity has the same exe, so a pin survives that upgrade.
/// With no pins this equals `items(model, desktop_only)`. A pin with no window left on this desktop stays, as a launcher.
pub fn items_with_pins(model: &WindowModel, pins: &Pins, desktop_only: bool) -> Vec<DockItem> {
    let mut running: Vec<Option<DockItem>> = items(model, desktop_only)
        .into_iter()
        .filter(|item| !pins.hides(&item.identity, item.executable.as_deref()))
        .map(Some)
        .collect();
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
                    badge: Badge::None,
                    members: Vec::new(),
                }),
        );
    }
    merged.extend(running.into_iter().flatten());
    if !pins.has_folders() {
        return merged;
    }
    let other_desktops = if desktop_only {
        items(model, false)
    } else {
        Vec::new()
    };
    let mut folded = Vec::with_capacity(merged.len());
    let mut claimed = Vec::new();
    for item in &merged {
        let pin = pinned_as(item, pins);
        if pin.as_ref().is_some_and(|pin| claimed.contains(pin)) {
            continue;
        }
        if let Some(folder) = pin.as_ref().and_then(|pin| pins.folder(pin)) {
            let members: Vec<_> = folder
                .members
                .iter()
                .filter_map(|identity| {
                    merged
                        .iter()
                        .find(|item| pinned_as(item, pins).as_ref() == Some(identity))
                        .cloned()
                        .map(|member| {
                            if member.windows.is_empty() {
                                other_desktops
                                    .iter()
                                    .find(|running| {
                                        pinned_as(running, pins).as_ref() == Some(identity)
                                    })
                                    .cloned()
                                    .unwrap_or(member)
                            } else {
                                member
                            }
                        })
                })
                .collect();
            claimed.extend(folder.members.iter().cloned());
            let mut tile = item.clone();
            tile.identity = pin.unwrap_or_else(|| item.identity.clone());
            tile.windows = members
                .iter()
                .flat_map(|item| item.windows.iter().copied())
                .collect();
            tile.active = members.iter().any(|item| item.active);
            tile.badge = members
                .iter()
                .map(|item| item.badge)
                .max()
                .unwrap_or_default();
            tile.members = members;
            folded.push(tile);
        } else {
            folded.push(item.clone());
        }
    }
    folded
}

/// Taskbar-style buttons beside the app slots (features D2). Start, Search and Task View sit left of the
/// slots; quick settings, the hidden-icons chevron and the show-desktop pill sit at the right end.
pub const BTN_DIPS: i32 = 44;
pub const SEP_DIPS: i32 = 8;
/// The show-desktop cell: a full button plus the end pad. Its pill hugs the dock's end and the click target runs
/// to the edge.
pub const STRIP_DIPS: i32 = BTN_DIPS + PAD_DIPS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Start,
    Search,
    TaskView,
    Desktop,
    /// Volume, Wi-Fi, Bluetooth and power mode in ROQA's own popover.
    Quick,
    /// The Recycle Bin and the drives, in ROQA's own popup.
    Places,
    /// Windows' own hidden-icons flyout, opened with Win+B and Enter and set beside the button.
    Tray,
}
impl Button {
    pub const ALL: [Button; 7] = [
        Button::Start,
        Button::Search,
        Button::TaskView,
        Button::Desktop,
        Button::Quick,
        Button::Places,
        Button::Tray,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    /// Settings store key.
    pub fn key(self) -> &'static str {
        [
            "dbstart", "dbsearch", "dbtask", "dbdesk", "dbquick", "dbplaces", "dbtray",
        ][self.index()]
    }
    /// The keys a click sends: the Windows key alone opens Start; the rest are shell shortcuts. None for the
    /// tray chevron, quick settings and places, which open their own popups.
    pub fn keys(self) -> Option<&'static [VIRTUAL_KEY]> {
        match self {
            Button::Start => Some(&[VK_LWIN]),
            Button::Search => Some(&[VK_LWIN, VK_S]),
            Button::TaskView => Some(&[VK_LWIN, VK_TAB]),
            Button::Desktop => Some(&[VK_LWIN, VK_D]),
            Button::Quick | Button::Places | Button::Tray => None,
        }
    }
}

/// Which buttons are on, and the dock's reading direction. Default: Start, Search, quick settings, the hidden-icons chevron and show-desktop; Task View off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Buttons {
    pub shown: [bool; 7],
    /// Mirrored for a right-to-left UI language: the dock runs from the right edge, as the taskbar does.
    /// The host sets it from the system; it is not a stored setting.
    pub rtl: bool,
    /// App slots to centre in a bar wider than they are; 0 leaves them at the start.
    /// The host sets it from the "Icons at the start" setting and the dock's slot count; it is not stored.
    pub centre: usize,
}
impl Buttons {
    #[cfg(test)]
    pub const NONE: Buttons = Buttons::of([false; 7]);
    pub const fn of(shown: [bool; 7]) -> Self {
        Self {
            shown,
            rtl: false,
            centre: 0,
        }
    }
    pub fn on(self, button: Button) -> bool {
        self.shown[button.index()]
    }
    /// Start, Search, Task View and Places, in that order.
    fn lead(self) -> impl Iterator<Item = Button> {
        [
            Button::Start,
            Button::Search,
            Button::TaskView,
            Button::Places,
        ]
        .into_iter()
        .filter(move |b| self.on(*b))
    }
    fn has_tail(self) -> bool {
        self.tail().next().is_some()
    }
    /// The hidden-icons chevron, quick settings, then the pill.
    fn tail(self) -> impl Iterator<Item = Button> {
        [Button::Tray, Button::Quick, Button::Desktop]
            .into_iter()
            .filter(move |b| self.on(*b))
    }
}
impl Default for Buttons {
    fn default() -> Self {
        Buttons::of([true, true, false, true, true, false, true])
    }
}

/// What a point is: an app slot or a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Slot(usize),
    Button(Button),
}
impl Hit {
    pub fn slot(self) -> Option<usize> {
        match self {
            Hit::Slot(slot) => Some(slot),
            Hit::Button(_) => None,
        }
    }
}

// The layout functions below are `#[inline(never)]`: each has many callers (paint, hit-testing, menus),
// and inlining every copy cost ~2.5 KB of release binary.

/// Cells in visual order: lead buttons, `slots` slots, tail buttons.
#[inline(never)]
pub fn cell_count(buttons: Buttons, slots: usize) -> usize {
    buttons.lead().count() + slots + buttons.tail().count()
}
#[inline(never)]
pub fn cell(buttons: Buttons, slots: usize, index: usize) -> Option<Hit> {
    let lead = buttons.lead().count();
    if index < lead {
        buttons.lead().nth(index).map(Hit::Button)
    } else if index < lead + slots {
        Some(Hit::Slot(index - lead))
    } else {
        buttons.tail().nth(index - lead - slots).map(Hit::Button)
    }
}
#[inline(never)]
pub fn cell_index(buttons: Buttons, slots: usize, hit: Hit) -> Option<usize> {
    let lead = buttons.lead().count();
    match hit {
        Hit::Slot(slot) => (slot < slots).then_some(lead + slot),
        Hit::Button(button) => match buttons.lead().position(|b| b == button) {
            Some(at) => Some(at),
            None => buttons
                .tail()
                .position(|b| b == button)
                .map(|at| lead + slots + at),
        },
    }
}

/// Whether the user's Windows display language reads right to left (Arabic, Hebrew, ...), which mirrors the
/// dock. Read once at startup: changing the display language needs a new sign-in.
pub fn system_rtl() -> bool {
    let mut layout = [0u16; 2]; // LOCALE_RETURN_NUMBER fills a DWORD
    // SAFETY: the buffer holds the DWORD the flag asks for; a language id is a valid locale id.
    let read = unsafe {
        GetLocaleInfoW(
            u32::from(GetUserDefaultUILanguage()),
            LOCALE_IREADINGLAYOUT | LOCALE_RETURN_NUMBER,
            Some(&mut layout),
        )
    };
    read > 0 && layout[0] == 1
}

/// How long the dock takes to slide up (or down) at its bottom edge.
pub const SLIDE_TIME: std::time::Duration = std::time::Duration::from_millis(180);

/// Pixels the dock content is pushed below its window at slide `progress` (0 hidden, 1 settled): the
/// whole `height` at 0 and nothing at 1. Only the window's rows from here down are visible and hit.
pub fn slide_offset(progress: f32, height: i32) -> i32 {
    ((1.0 - progress.clamp(0.0, 1.0)) * height as f32).round() as i32
}

pub fn px(dips: i32, dpi: u32) -> i32 {
    ((i64::from(dips) * i64::from(dpi.max(1)) + 48) / 96) as i32
}

/// Window x of the first slot, and the width the tail takes at the right end (pixels).
#[inline(never)]
fn lead_px(buttons: Buttons, dpi: u32) -> i32 {
    let lead = buttons.lead().count() as i32;
    px(PAD_DIPS, dpi)
        + if lead > 0 {
            lead * px(BTN_DIPS, dpi) + px(SEP_DIPS, dpi)
        } else {
            0
        }
}
#[inline(never)]
fn tail_px(buttons: Buttons, dpi: u32) -> i32 {
    if !buttons.has_tail() {
        return px(PAD_DIPS, dpi);
    }
    px(SEP_DIPS, dpi)
        + (buttons.tail().count() as i32 - i32::from(buttons.on(Button::Desktop)))
            * px(BTN_DIPS, dpi)
        + px(
            if buttons.on(Button::Desktop) {
                STRIP_DIPS
            } else {
                PAD_DIPS
            },
            dpi,
        )
}

/// How far a wide bar pushes the slots right so they sit in the middle of the room between the buttons.
fn centre_px(buttons: Buttons, width_px: i32, dpi: u32) -> i32 {
    let room = width_px - lead_px(buttons, dpi) - tail_px(buttons, dpi);
    let used = px(SLOT_DIPS, dpi) * buttons.centre as i32;
    if buttons.centre == 0 {
        0
    } else {
        ((room - used) / 2).max(0)
    }
}

/// Total width in pixels, the sum of the scaled parts so no cell is clipped by rounding. With no buttons:
/// the pad, the slots and the pad.
#[inline(never)]
pub fn width_px(buttons: Buttons, slots: usize, dpi: u32) -> i32 {
    lead_px(buttons, dpi) + px(SLOT_DIPS, dpi) * slots.max(1) as i32 + tail_px(buttons, dpi)
}

/// Window-pixel span `(left, width)` of a slot or button in a dock `width_px` wide.
pub fn span(buttons: Buttons, width_px: i32, dpi: u32, hit: Hit) -> (i32, i32) {
    let (left, cell) = ltr_span(buttons, width_px, dpi, hit);
    (
        if buttons.rtl {
            width_px - left - cell
        } else {
            left
        },
        cell,
    )
}

/// Window x as the left-to-right layout sees it: a mirrored dock flips pixel columns.
fn ltr_x(buttons: Buttons, x_px: i32, width_px: i32) -> i32 {
    if buttons.rtl {
        width_px - 1 - x_px
    } else {
        x_px
    }
}

/// `span` of the left-to-right layout; the others mirror it.
#[inline(never)]
fn ltr_span(buttons: Buttons, width_px: i32, dpi: u32, hit: Hit) -> (i32, i32) {
    let (btn, strip) = (px(BTN_DIPS, dpi), px(STRIP_DIPS, dpi));
    match hit {
        Hit::Slot(slot) => (
            lead_px(buttons, dpi)
                + centre_px(buttons, width_px, dpi)
                + px(SLOT_DIPS, dpi) * slot as i32,
            px(SLOT_DIPS, dpi),
        ),
        Hit::Button(Button::Desktop) => (width_px - strip, strip),
        Hit::Button(button @ (Button::Quick | Button::Tray)) => {
            let end = if buttons.on(Button::Desktop) {
                STRIP_DIPS
            } else {
                PAD_DIPS
            };
            // Each button sits left of the tail buttons after it (all but the pill).
            let beside = buttons
                .tail()
                .skip_while(|b| *b != button)
                .skip(1)
                .filter(|b| *b != Button::Desktop)
                .count() as i32;
            (width_px - px(end, dpi) - btn - beside * btn, btn)
        }
        Hit::Button(button) => {
            let at = buttons.lead().position(|b| b == button).unwrap_or(0) as i32;
            (px(PAD_DIPS, dpi) + at * btn, btn)
        }
    }
}

/// The centre of dock slot `slot` for a dock `width_px` wide whose left edge is `dock_x`.
pub fn slot_center_x(dock_x: i32, buttons: Buttons, width_px: i32, slot: usize, dpi: u32) -> i32 {
    let (left, width) = span(buttons, width_px, dpi, Hit::Slot(slot));
    dock_x + left + width / 2
}

/// Slots that fit inside the realized window width; the rest are clipped.
#[inline(never)]
pub fn visible_slots(buttons: Buttons, width_px: i32, dpi: u32, slots: usize) -> usize {
    let room = width_px - lead_px(buttons, dpi) - tail_px(buttons, dpi);
    slots.min((room / px(SLOT_DIPS, dpi).max(1)).max(0) as usize)
}

/// What is under window x `x_px`: a button, or a fully visible slot.
#[inline(never)]
pub fn hit_at(buttons: Buttons, x_px: i32, width_px: i32, dpi: u32, slots: usize) -> Option<Hit> {
    let x_px = ltr_x(buttons, x_px, width_px);
    let on_button = Button::ALL
        .into_iter()
        .filter(|b| buttons.on(*b))
        .find(|b| {
            let (left, width) = ltr_span(buttons, width_px, dpi, Hit::Button(*b));
            (left..left + width).contains(&x_px)
        });
    if let Some(button) = on_button {
        return Some(Hit::Button(button));
    }
    let offset = x_px - lead_px(buttons, dpi) - centre_px(buttons, width_px, dpi);
    if offset < 0 {
        return None;
    }
    let slot = (offset / px(SLOT_DIPS, dpi).max(1)) as usize;
    (slot < visible_slots(buttons, width_px, dpi, slots)).then_some(Hit::Slot(slot))
}

/// Window x of the hairline between the lead buttons and the slots, and between the slots and the tail,
/// for the groups that exist.
pub fn separators(buttons: Buttons, width_px: i32, dpi: u32) -> [Option<i32>; 2] {
    let at = |x| if buttons.rtl { width_px - x } else { x };
    // Each hairline sits midway between the two neighbours' visible edges: glyphs have wider margins than
    // 40 DIP icons, so it leans toward the buttons on the lead side and falls on the cell start on the tail.
    [
        (buttons.lead().count() > 0).then(|| at(lead_px(buttons, dpi) - px(6, dpi))),
        buttons
            .has_tail()
            .then(|| at(width_px - tail_px(buttons, dpi) + px(SEP_DIPS, dpi))),
    ]
}

/// How long a file dragged over a dock slot rests there before that app's window comes forward.
pub const RAISE_DELAY: std::time::Duration = std::time::Duration::from_millis(600);

/// Pointer travel (DIPs) before a press on a pinned slot becomes a reorder drag instead of a click.
pub const DRAG_DIPS: i32 = 6;

pub fn folder_drop(offset: i32, width: i32, dpi: u32) -> bool {
    (offset - width / 2).abs() <= px(14, dpi)
}

/// A folder tile's 2x2 grid cell (left, top, side as fractions of the tile): a 0.13 inset, 0.08 gutter.
pub fn mini_cell(index: usize) -> Option<(f32, f32, f32)> {
    (index < 4).then_some({
        (
            0.13 + (index % 2) as f32 * 0.41,
            0.13 + (index / 2) as f32 * 0.41,
            0.33,
        )
    })
}

pub fn drag_started(origin_x: i32, x: i32, dpi: u32) -> bool {
    (x - origin_x).abs() >= px(DRAG_DIPS, dpi)
}

/// Pin index a drag released at `x_px` drops on: the slot under the pointer (clamped to the visible pins, so
/// releasing beyond either end drops on the first or last), or `None` with no visible pin. Item `i` is pin `i`.
pub fn drop_index(
    buttons: Buttons,
    x_px: i32,
    width_px: i32,
    dpi: u32,
    slots: usize,
    pins: usize,
) -> Option<usize> {
    let last = visible_slots(buttons, width_px, dpi, slots)
        .min(pins)
        .checked_sub(1)?;
    let x_px = ltr_x(buttons, x_px, width_px);
    let slot = (x_px - lead_px(buttons, dpi)).div_euclid(px(SLOT_DIPS, dpi).max(1));
    Some(slot.clamp(0, last as i32) as usize)
}

/// How far item `index` sits from its place, in slots (+1 toward the end), while item `from` is held and the gap
/// is at `gap` (a slot position that eases between slots): items between the held one and the gap close up
/// around it and the gap opens at its end.
pub fn drag_shift(from: usize, gap: f32, index: usize) -> f32 {
    let at = index as f32;
    match index.cmp(&from) {
        std::cmp::Ordering::Greater => -(gap - at + 1.0).clamp(0.0, 1.0),
        std::cmp::Ordering::Less => (at - gap + 1.0).clamp(0.0, 1.0),
        std::cmp::Ordering::Equal => 0.0,
    }
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

pub fn open_action(item: &DockItem) -> Option<Action> {
    item.windows
        .first()
        .copied()
        .map(Action::Activate)
        .or_else(|| pins::launch_target(&item.identity).map(Action::Launch))
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
    launch_with(target, None)
}

/// `launch` with command-line `parameters`, for the file shelf's reveal and files dropped on a pin. A UNC path or
/// mapped drive can stall the shell on a dead share: it starts on the Shelf's worker, so `true` means asked, not
/// started, and `false` that the worker is busy or could not start.
pub fn launch_with(target: &str, parameters: Option<&str>) -> bool {
    if crate::shelf_os::is_network(target) {
        return crate::shelf_os::open_network(target, parameters);
    }
    shell(target, parameters, false)
}

/// The shell call itself. `wait` is for a short-lived thread with no message loop (the network worker), where the
/// call must finish its launch before the thread ends (`SEE_MASK_NOASYNC`).
pub fn shell(target: &str, parameters: Option<&str>, wait: bool) -> bool {
    let file = HSTRING::from(target);
    let parameters = parameters.map(HSTRING::from);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI | if wait { SEE_MASK_NOASYNC } else { 0 },
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: parameters
            .as_ref()
            .map_or(PCWSTR::null(), |text| PCWSTR(text.as_ptr())),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `info`, `file` and `parameters` outlive the call; no process handle is requested, so nothing is left to close.
    unsafe { ShellExecuteExW(&mut info).is_ok() }
}

/// Taps `keys` together (down in order, up in reverse) with one `SendInput`, on the user's click only. No
/// hook and nothing stays pressed. False when Windows injected fewer events than asked.
pub fn send_keys(keys: &[VIRTUAL_KEY]) -> bool {
    let events: Vec<INPUT> = keys
        .iter()
        .map(|&vk| key_event(vk, false))
        .chain(keys.iter().rev().map(|&vk| key_event(vk, true)))
        .collect();
    send_events(&events)
}

/// Lifts `keys` (key-up only) while the user still holds them, so the keys sent next are not combined with them.
pub fn release_keys(keys: &[VIRTUAL_KEY]) -> bool {
    let events: Vec<INPUT> = keys.iter().map(|&vk| key_event(vk, true)).collect();
    send_events(&events)
}

fn key_event(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                ..Default::default()
            },
        },
    }
}

fn send_events(events: &[INPUT]) -> bool {
    // SAFETY: `events` is a live slice of fully initialized INPUT structs and the size is theirs.
    unsafe { SendInput(events, std::mem::size_of::<INPUT>() as i32) as usize == events.len() }
}

/// The visible top-level window of `class`. `FindWindow` cannot see the shell's popups, so the windows are enumerated.
fn find_popup(class: &str) -> Option<HWND> {
    unsafe extern "system" fn visit(hwnd: HWND, parameter: LPARAM) -> BOOL {
        // SAFETY: `parameter` is the tuple below, alive for the whole synchronous EnumWindows call.
        let (class, found) = unsafe { &mut *(parameter.0 as *mut (&str, Option<HWND>)) };
        let mut name = [0u16; 64];
        // SAFETY: scalar window queries on a handle EnumWindows just gave and a local buffer.
        let (len, shown) = unsafe { (GetClassNameW(hwnd, &mut name), IsWindowVisible(hwnd)) };
        let hit =
            shown.as_bool() && String::from_utf16_lossy(&name[..len.max(0) as usize]) == *class;
        if hit {
            *found = Some(hwnd);
        }
        BOOL(i32::from(!hit))
    }
    let mut search = (class, None);
    // SAFETY: the callback does not keep the pointer and cannot panic.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut _ as isize)) };
    search.1
}

/// How long the shell is given to move focus to the tray (after Win+B) and to draw the flyout or menu (after Enter, Win+X).
const TRAY_WAIT: std::time::Duration = std::time::Duration::from_millis(200);
/// How long after the click the Start menu is put back where it belongs, as Windows places it again while it opens.
const START_WATCH: std::time::Duration = std::time::Duration::from_millis(2500);
/// The class of the menu Win+X opens.
const START_POPUP: &str = "Xaml_WindowedPopupClass";

/// The window class of `window`.
fn class_of(window: HWND) -> String {
    let mut class = [0u16; 64];
    // SAFETY: a scalar window query into a local buffer.
    let len = unsafe { GetClassNameW(window, &mut class) }.max(0) as usize;
    String::from_utf16_lossy(&class[..len])
}

/// Where the Start menu goes: its left edge on `anchor.0`, its bottom on `anchor.1`, inside `work`.
#[derive(Clone, Copy)]
struct Aim {
    anchor: (i32, i32),
    work: crate::layout::Bounds,
}

thread_local! {
    /// The Start menu's target, read by the WinEvent callback on the thread that placed the hook.
    static AIM: std::cell::Cell<Option<Aim>> = const { std::cell::Cell::new(None) };
}

/// Sets `flyout` at `x` (`left`, or its centre on `anchor.0` for the tray) above `anchor.1`, inside `work`; only
/// when its class is `popup`, so no other window is ever moved.
fn place_popup(
    flyout: HWND,
    popup: &str,
    start: bool,
    anchor: (i32, i32),
    work: crate::layout::Bounds,
) {
    let mut rect = RECT::default();
    // SAFETY: scalar window queries and one move of a foreign window revalidated by its class name.
    unsafe {
        if class_of(flyout) != popup || GetWindowRect(flyout, &mut rect).is_err() {
            return;
        }
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        let left = if start {
            anchor.0
        } else {
            anchor.0 - width / 2
        };
        let x = left.clamp(work.x, (work.x + work.width - width).max(work.x));
        let y = (anchor.1 - height).max(work.y);
        if (rect.left, rect.top) != (x, y) {
            let _ = SetWindowPos(
                flyout,
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

/// A window of any process appeared or moved: the Start menu is set back at its place (a move by `place_popup`
/// reports here once more and is found in place).
unsafe extern "system" fn start_moved(
    _: HWINEVENTHOOK,
    _: u32,
    window: HWND,
    object: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if object == 0
        && let Some(aim) = AIM.get()
    {
        place_popup(window, START_POPUP, true, aim.anchor, aim.work);
    }
}

/// Opens Windows' own popup for a dock button on the user's click (hidden icons: Win+B, then Enter; Start: the
/// Win+X power-user menu; Windows draws it and handles every click in it) and sets it above `anchor` (the button's
/// screen x, the dock's top y), inside `work`: centred on the hidden-icons button, left edges level on Start.
/// A thread that lives about half a second (the Start menu: two and a half, listening for window events only) waits
/// out the shell's focus moves; nothing runs between clicks. The popup is only moved when its class is the expected one.
pub fn open_flyout(button: Button, anchor: (i32, i32), work: crate::layout::Bounds) {
    let start = button == Button::Start;
    let _ = std::thread::Builder::new().spawn(move || {
        if start {
            if send_keys(&[VK_LWIN, VK_X]) {
                watch_start(Aim { anchor, work });
            }
            return;
        }
        if !send_keys(&[VK_LWIN, VK_B]) {
            return;
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "the tray needs a moment after the key press, on a worker"
        )]
        std::thread::sleep(TRAY_WAIT);
        // Enter must reach the taskbar: when the shell was slow to take the focus it would land in the user's app.
        // SAFETY: a scalar query.
        let tray = class_of(unsafe { GetForegroundWindow() });
        if matches!(tray.as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
            && send_keys(&[VK_RETURN])
        {
            #[expect(
                clippy::disallowed_methods,
                reason = "the tray needs a moment after the key press, on a worker"
            )]
            std::thread::sleep(TRAY_WAIT);
            // SAFETY: a scalar query.
            place_popup(
                unsafe { GetForegroundWindow() },
                "TopLevelWindowForOverflowXamlIsland",
                false,
                anchor,
                work,
            );
        }
    });
}

/// Puts the Start menu at `aim` as soon as it shows and each time Windows moves it, for `START_WATCH`: the thread
/// sleeps in its message wait and wakes only for the window events that reach it (no polling).
fn watch_start(aim: Aim) {
    use std::time::Instant;
    use windows::Win32::UI::{
        Accessibility::{SetWinEventHook, UnhookWinEvent},
        WindowsAndMessaging::{
            DispatchMessageW, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, MWMO_INPUTAVAILABLE,
            MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
            WINEVENT_OUTOFCONTEXT,
        },
    };
    AIM.set(Some(aim));
    // SAFETY: an out-of-context hook delivered to this thread's message loop below, removed before it ends; the
    // callback only reads this thread's `AIM` and moves a window after checking its class.
    let hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_LOCATIONCHANGE,
            None,
            Some(start_moved),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if let Some(menu) = find_popup(START_POPUP) {
        place_popup(menu, START_POPUP, true, aim.anchor, aim.work);
    }
    let end = Instant::now() + START_WATCH;
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        let mut message = Default::default();
        // SAFETY: waits for this thread's own input (the hook's events); the messages drained are dispatched.
        unsafe {
            MsgWaitForMultipleObjectsEx(
                None,
                left.as_millis() as u32,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            );
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&message);
            }
        }
    }
    // SAFETY: unhooks the hook set above, on the thread that set it.
    unsafe {
        let _ = UnhookWinEvent(hook);
    }
    AIM.set(None);
}
/// Asks each window to close (`WM_CLOSE`, posted: an app may refuse or prompt, and ROQA never waits).
pub fn close(windows: &[WindowKey]) {
    for key in windows.iter().filter(|&&key| valid_target(key)) {
        // SAFETY: a scalar post to a revalidated foreign HWND.
        let _ = unsafe {
            PostMessageW(
                Some(HWND(key.handle as *mut _)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            )
        };
    }
}

/// Whether the window is kept above all others right now.
pub fn is_on_top(key: WindowKey) -> bool {
    // SAFETY: a scalar style read on a revalidated foreign HWND.
    valid_target(key)
        && unsafe { GetWindowLongPtrW(HWND(key.handle as *mut _), GWL_EXSTYLE) } as u32
            & WS_EX_TOPMOST.0
            != 0
}

/// Keeps the window above all others, or lets it fall back. Windows may decline (an elevated window).
pub fn set_on_top(key: WindowKey, on: bool) {
    if !valid_target(key) {
        return;
    }
    let order = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
    // SAFETY: a z-order change only (no move, size or activation) on a revalidated foreign HWND.
    let _ = unsafe {
        SetWindowPos(
            HWND(key.handle as *mut _),
            Some(order),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
}

/// Executes a dock action and describes the recovery when Windows declines it.
pub fn action_failure(action: Action) -> Option<&'static str> {
    let (succeeded, message) = match action {
        Action::Launch(target) => (launch(&target), "Windows couldn't open this app."),
        Action::Activate(_) => (perform(action), "Can't switch. Try the Windows taskbar."),
        Action::Minimize(_) => (perform(action), "Windows couldn't minimize this window."),
    };
    (!succeeded).then_some(message)
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
    use crate::motion::{SETTLE, Tween};
    use std::time::Instant;

    #[test]
    fn slide_offset_runs_from_the_full_height_to_zero_and_stays_inside_the_window() {
        assert_eq!(super::slide_offset(0.0, 64), 64);
        assert_eq!(super::slide_offset(1.0, 64), 0);
        assert_eq!(super::slide_offset(0.5, 64), 32);
        // The easing may leave 0..1 by float error; the content never rises above or sinks below the window.
        assert_eq!(
            (super::slide_offset(-0.3, 64), super::slide_offset(1.4, 64)),
            (64, 0)
        );
        assert_eq!(super::slide_offset(0.5, 0), 0);
        let steps: Vec<i32> = (0..=10)
            .map(|i| super::slide_offset(i as f32 / 10.0, 63))
            .collect();
        assert!(steps.windows(2).all(|pair| pair[0] >= pair[1]));
    }

    #[test]
    fn the_slide_timeline_starts_hidden_ends_settled_and_is_idle_afterwards() {
        let mut slide = Tween::rest(0.0);
        let start = Instant::now();
        assert_eq!(super::slide_offset(slide.value(start), 64), 64);
        slide.retarget(1.0, super::SLIDE_TIME, SETTLE);
        // Past its end the tween sits exactly on 1 and reports no running frames, so nothing keeps ticking.
        let end = Instant::now() + super::SLIDE_TIME * 2;
        assert_eq!(slide.value(end), 1.0);
        assert!(!slide.running(end));
        assert_eq!(super::slide_offset(slide.value(end), 64), 0);
        // Leaving again starts from where it is, so an interruption never jumps.
        slide.retarget(0.0, super::SLIDE_TIME, SETTLE);
        assert!(slide.running(Instant::now()));
    }

    use super::*;
    use crate::window_model::WindowRecord;

    #[test]
    fn folder_open_focuses_running_members_without_minimizing_or_launching_again() {
        let mut pins = Pins::default();
        let exe = Identity::Executable(r"c:\vendor.exe".into());
        pins.group(&exe, &Identity::Application("Idle".into()));
        let mut window = record(1, Identity::Application("Vendor!App".into()), true);
        window.executable = Some(r"c:\vendor.exe".into());
        window.current_desktop = false;
        let model = WindowModel {
            windows: vec![window],
            ..Default::default()
        };
        let tiles = items_with_pins(&model, &pins, true);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].members.len(), 2);
        let running = tiles[0]
            .members
            .iter()
            .find(|member| !member.windows.is_empty())
            .unwrap();
        assert_eq!(open_action(running), Some(Action::Activate(key(1))));
        let idle = tiles[0]
            .members
            .iter()
            .find(|member| member.windows.is_empty())
            .unwrap();
        assert!(matches!(open_action(idle), Some(Action::Launch(_))));
        assert_eq!(pins.folder(&tiles[0].identity).unwrap().members.len(), 2);
    }

    #[test]
    fn cross_desktop_folder_claims_canonical_pins_in_either_order() {
        let executable = Identity::Executable(r"c:\vendor.exe".into());
        let idle = Identity::Application("Idle".into());
        let mut window = record(1, Identity::Application("Vendor!App".into()), true);
        window.executable = Some(r"c:\vendor.exe".into());
        window.current_desktop = false;
        let model = WindowModel {
            windows: vec![window],
            ..Default::default()
        };
        for (source, target) in [(&executable, &idle), (&idle, &executable)] {
            let mut pins = Pins::default();
            assert!(pins.group(source, target));
            let tiles = items_with_pins(&model, &pins, true);
            assert_eq!(tiles.len(), 1);
            assert_eq!(tiles[0].members.len(), 2);
            assert_eq!(tiles[0].windows, [key(1)]);
            assert_eq!(
                tiles[0]
                    .members
                    .iter()
                    .filter(|member| !member.windows.is_empty())
                    .count(),
                1
            );
        }
    }

    #[test]
    fn folder_drop_has_a_center_target_and_reorder_edges_at_every_scale() {
        for dpi in [96, 120, 144, 192] {
            let width = px(SLOT_DIPS, dpi);
            assert!(folder_drop(width / 2, width, dpi));
            assert!(!folder_drop(0, width, dpi));
            assert!(!folder_drop(width - 1, width, dpi));
        }
        let previews: Vec<_> = (0..4).map(|index| mini_cell(index).unwrap()).collect();
        assert!(mini_cell(4).is_none());
        assert!(previews.iter().all(|&(left, top, side)| left >= 0.0
            && top >= 0.0
            && left + side <= 1.0
            && top + side <= 1.0));
        assert!(previews[0].0 + previews[0].2 < previews[1].0);
        assert!(previews[0].1 + previews[0].2 < previews[2].1);
    }

    #[test]
    fn folders_fold_badges_and_windows_but_keep_every_launcher() {
        let mut pins = Pins::default();
        pins.group(
            &Identity::Application("A".into()),
            &Identity::Application("B".into()),
        );
        pins.add(Identity::Application("C".into())).unwrap();
        let mut window = record(1, Identity::Application("B".into()), true);
        window.title = "Inbox (7)".into();
        let model = WindowModel {
            windows: vec![window],
            ..Default::default()
        };
        let folded = items_with_pins(&model, &pins, false);
        assert_eq!(folded.len(), 2);
        assert_eq!(folded[0].members.len(), 2);
        assert_eq!(folded[0].windows, [key(1)]);
        assert!(folded[0].active);
        assert_eq!(folded[0].badge, Badge::Count(7));
        assert!(
            folded[0]
                .members
                .iter()
                .any(|member| member.windows.is_empty())
        );
    }

    const N: Buttons = Buttons::NONE;
    fn slot_at(x: i32, width: i32, dpi: u32, slots: usize) -> Option<usize> {
        hit_at(N, x, width, dpi, slots).and_then(Hit::slot)
    }

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
            current_desktop: true,
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
                record(5, exe, true),
                record(7, Identity::Unavailable(7), false),
            ],
            incomplete: None,
        };
        let items = items(&model, false);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].identity, app);
        assert_eq!(items[1].windows, vec![key(5), key(9)]);
        assert!(items[1].active && !items[0].active);
        assert_eq!(items[2].identity, Identity::Unavailable(7));
        assert!(super::items(&WindowModel::default(), false).is_empty());
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
        assert_eq!(
            items_with_pins(&model, &Pins::default(), false),
            items(&model, false)
        );
        assert!(items_with_pins(&WindowModel::default(), &Pins::default(), false).is_empty());
    }

    #[test]
    fn a_hidden_app_leaves_the_dock_and_comes_back() {
        let model = sample_model();
        let mut pins = Pins::default();
        assert!(pins.hide(Identity::Executable("c:\\9.exe".into())));
        let shown = items_with_pins(&model, &pins, false);
        assert_eq!(shown.len(), items(&model, false).len() - 1);
        assert!(
            shown
                .iter()
                .all(|item| item.executable.as_deref() != Some("c:\\9.exe"))
        );
        assert!(pins.unhide(0));
        assert_eq!(items_with_pins(&model, &pins, false), items(&model, false));
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
        let merged = items_with_pins(&model, &pins_of(&pinned), false);
        let ids: Vec<_> = merged.iter().map(|i| i.identity.clone()).collect();
        // Pin order, then unpinned running (5.exe, Unavailable) in the old identity order.
        assert_eq!(
            ids,
            [
                pinned[0].clone(),
                missing,
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
        assert_eq!(merged, items_with_pins(&model, &pins_of(&pinned), false));
        let mut fewer = sample_model();
        fewer.windows.retain(|w| w.key != key(9));
        let ids: Vec<_> = items_with_pins(&fewer, &pins_of(&pinned), false)
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
            false,
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
        let merged = items_with_pins(&model, &pins, false);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].windows, vec![key(6)]);
        assert_eq!(merged[1].windows, vec![key(4)]);
    }

    #[test]
    fn desktop_filter_drops_other_desktop_windows_but_keeps_pins() {
        let mut model = sample_model();
        // 9.exe (pinned) and 5.exe live only on another desktop; Mail (3) and 7 are here.
        for window in &mut model.windows {
            window.current_desktop = !matches!(window.key.handle, 9 | 5);
        }
        let pin = Identity::Executable("c:\\9.exe".into());
        let pins = pins_of(std::slice::from_ref(&pin));
        let ids = |desktop_only| -> Vec<_> {
            items_with_pins(&model, &pins, desktop_only)
                .into_iter()
                .map(|item| (item.identity, item.windows.len()))
                .collect()
        };
        // Off: everything, pin first. On: the pin stays as a launcher (no windows), 5.exe is gone.
        assert_eq!(ids(false).len(), 4);
        assert_eq!(
            ids(true),
            [
                (pin, 0),
                (Identity::Application("Contoso.Mail".into()), 1),
                (Identity::Unavailable(7), 1)
            ]
        );
        // A group keeps the windows that are here: only the other desktop's one leaves.
        model.windows.push(record(
            8,
            Identity::Application("Contoso.Mail".into()),
            false,
        ));
        model.windows[4].current_desktop = false;
        let mail = items(&model, true)
            .into_iter()
            .find(|item| item.identity == Identity::Application("Contoso.Mail".into()))
            .unwrap();
        assert_eq!(mail.windows, vec![key(3)]);
        assert_eq!(items(&model, false).len(), 4);
    }

    #[test]
    fn launcher_items_get_names_and_a_launch_action() {
        let pins = pins_of(&[
            Identity::Application("Contoso.Mail_abc!App".into()),
            Identity::Executable("c:\\tools\\edit.exe".into()),
        ]);
        let model = WindowModel::default();
        let merged = items_with_pins(&model, &pins, false);
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
    fn a_failed_activation_points_to_the_windows_taskbar() {
        let missing = WindowKey {
            handle: 0,
            process: 0,
        };
        assert_eq!(
            action_failure(Action::Activate(missing)),
            Some("Can't switch. Try the Windows taskbar.")
        );
        assert_eq!(
            action_failure(Action::Minimize(missing)),
            Some("Windows couldn't minimize this window.")
        );
    }

    #[test]
    fn title_indicators_and_group_badges() {
        assert_eq!(title_badge("(3) Mail"), Badge::Count(3));
        assert_eq!(title_badge("Chat [2]"), Badge::Count(2));
        assert_eq!(title_badge("(99+) Chat"), Badge::Count(100));
        assert_eq!(title_badge("? Downloads"), Badge::Dot);
        for title in [
            "Mail (0)",
            "Report (2026) draft",
            "[abc] chat",
            "(3x) chat",
            "hello",
        ] {
            assert_eq!(title_badge(title), Badge::None, "{title}");
        }
        assert_eq!(title_badge("(999999999999) Mail"), Badge::Count(100));
        let id = Identity::Unavailable(1);
        let make = |handle, title: &str, current_desktop| WindowRecord {
            key: WindowKey { handle, process: 1 },
            title: title.into(),
            identity: id.clone(),
            executable: None,
            minimized: false,
            foreground: false,
            current_desktop,
        };
        let model = WindowModel {
            windows: vec![make(1, "(3) A", true), make(2, "[7] B", false)],
            ..Default::default()
        };
        assert_eq!(items(&model, false)[0].badge, Badge::Count(7));
        assert_eq!(items(&model, true)[0].badge, Badge::Count(3));
    }

    #[test]
    fn click_activates_cycles_and_minimizes_like_a_taskbar() {
        let item = |handles: &[usize]| DockItem {
            members: Vec::new(),
            identity: Identity::Unavailable(0),
            executable: None,
            windows: handles.iter().map(|h| key(*h)).collect(),
            active: false,
            badge: Badge::None,
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
        assert_eq!(width_px(N, 3, 144), 276);
        assert_eq!(slot_at(11, 276, 144, 3), None);
        assert_eq!(slot_at(12, 276, 144, 3), Some(0));
        assert_eq!(slot_at(95, 276, 144, 3), Some(0));
        assert_eq!(slot_at(96, 276, 144, 3), Some(1));
        assert_eq!(slot_at(263, 276, 144, 3), Some(2));
        assert_eq!(slot_at(264, 276, 144, 3), None);
        // Work area narrower than the content: only fully visible slots are hit targets.
        assert_eq!(visible_slots(N, 200, 144, 3), 2);
        assert_eq!(slot_at(190, 200, 144, 3), None);
        assert_eq!(width_px(N, 0, 96), width_px(N, 1, 96));
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
        // 96 dpi: pad 8, slot 56; item i is slot i at x = 8 + 56 * i.
        let at = |x, slots, pins| drop_index(N, x, width_px(N, slots, 96), 96, slots, pins);
        // 4 pins among 7 slots (4 pins, 3 running): slots 0..=3 are pins.
        assert_eq!(at(8, 7, 4), Some(0));
        assert_eq!(at(8 + 56 * 2 + 55, 7, 4), Some(2));
        assert_eq!(at(8 + 56 * 3, 7, 4), Some(3));
        // Left of the bar, or beyond the pins: first and last pin.
        assert_eq!(at(-300, 7, 4), Some(0));
        assert_eq!(at(8 + 56 * 6, 7, 4), Some(3));
        assert_eq!(at(10_000, 7, 4), Some(3));
        // No pins: nothing to drop on.
        assert_eq!(at(70, 7, 0), None);
        // Clipped bar (width for 2 slots, so only 2 pins visible): the drop never lands on a hidden pin.
        assert_eq!(drop_index(N, 10_000, width_px(N, 2, 96), 96, 7, 4), Some(1));
        // Agrees with `slot_at`: the slot a click would hit is the slot a drop lands on.
        let width = width_px(N, 7, 96);
        for x in 0..width {
            if let Some(slot @ 0..=3) = slot_at(x, width, 96, 7) {
                assert_eq!(drop_index(N, x, width, 96, 7, 4), Some(slot), "x {x}");
            }
        }
        // 150%: pad 12, slot 84.
        assert_eq!(
            drop_index(N, 12 + 84 * 2, width_px(N, 5, 144), 144, 5, 3),
            Some(2)
        );
    }

    #[test]
    fn drag_shift_opens_the_gap_at_either_end_and_in_a_one_icon_dock() {
        // Holding item 1 of 4, gap at rest on 1: nothing moves.
        assert!((0..4).all(|i| drag_shift(1, 1.0, i) == 0.0));
        // Gap dragged right to 3: items 2 and 3 close left, item 0 stays.
        let to = |gap| [0, 2, 3].map(|i| drag_shift(1, gap, i));
        assert_eq!(to(3.0), [0.0, -1.0, -1.0]);
        // Gap dragged left to 0: item 0 steps right, the later items stay.
        assert_eq!(drag_shift(1, 0.0, 0), 1.0);
        assert_eq!(drag_shift(1, 0.0, 2), 0.0);
        // Halfway between slots is halfway shifted.
        assert_eq!(drag_shift(1, 1.5, 2), -0.5);
        // A one-icon dock has nothing to shift.
        assert_eq!(drag_shift(0, 0.0, 0), 0.0);
    }

    #[test]
    fn pin_toggle_target_follows_the_claiming_pin() {
        let running = |identity, exe: &str| DockItem {
            members: Vec::new(),
            identity,
            executable: Some(exe.into()),
            windows: vec![key(1)],
            active: false,
            badge: Badge::None,
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
            members: Vec::new(),
            identity: Identity::Application("Gone".into()),
            executable: None,
            windows: vec![],
            active: false,
            badge: Badge::None,
        };
        assert_eq!(pinned_as(&gone, &pins), None);
        let unavailable = running(Identity::Unavailable(3), r"c:\other.exe");
        assert_eq!(pinned_as(&unavailable, &pins), None);
    }

    fn only(button: Button) -> Buttons {
        let mut on = [false; 7];
        on[button.index()] = true;
        Buttons::of(on)
    }
    const ALL: Buttons = Buttons::of([true; 7]);

    #[test]
    fn width_grows_by_each_button_and_none_keeps_the_old_dock() {
        let base = width_px(N, 3, 96);
        assert_eq!(base, PAD_DIPS * 2 + SLOT_DIPS * 3);
        // Lead buttons share one separator; the tail has one separator, the hidden-icons chevron and the strip.
        assert_eq!(
            width_px(only(Button::Start), 3, 96),
            base + BTN_DIPS + SEP_DIPS
        );
        assert_eq!(
            width_px(only(Button::TaskView), 3, 96),
            base + BTN_DIPS + SEP_DIPS
        );
        assert_eq!(
            width_px(only(Button::Tray), 3, 96),
            base + SEP_DIPS + BTN_DIPS
        );
        assert_eq!(
            width_px(only(Button::Desktop), 3, 96),
            base - PAD_DIPS + SEP_DIPS + STRIP_DIPS
        );
        assert_eq!(
            width_px(ALL, 3, 96),
            PAD_DIPS
                + 4 * BTN_DIPS
                + SEP_DIPS
                + SLOT_DIPS * 3
                + SEP_DIPS
                + 2 * BTN_DIPS
                + STRIP_DIPS
        );
        assert_eq!(
            Buttons::default().shown,
            [true, true, false, true, true, false, true]
        );
    }

    #[test]
    fn cells_never_overlap_and_tile_the_dock_at_any_scale() {
        for dpi in [96, 120, 144, 192] {
            for mask in 0..64u32 {
                let b = Buttons::of(std::array::from_fn(|i| mask >> i & 1 == 1));
                let slots = 4;
                let width = width_px(b, slots, dpi);
                assert_eq!(cell_count(b, slots), slots + mask.count_ones() as usize);
                let mut end = 0;
                for index in 0..cell_count(b, slots) {
                    let hit = cell(b, slots, index).unwrap();
                    assert_eq!(cell_index(b, slots, hit), Some(index));
                    let (left, w) = span(b, width, dpi, hit);
                    assert!(left >= end && w > 0, "mask {mask} dpi {dpi} cell {index}");
                    end = left + w;
                    // The centre of every cell hits that cell.
                    assert_eq!(hit_at(b, left + w / 2, width, dpi, slots), Some(hit));
                }
                assert!(end <= width);
                assert_eq!(cell(b, slots, cell_count(b, slots)), None);
            }
        }
    }

    #[test]
    fn hit_testing_maps_buttons_slots_and_gaps() {
        // 96 dpi, Start + Search + hidden-icons chevron + strip, 2 slots: pad 8, buttons 44, sep 8, slots 56.
        let b = Buttons::of([true, true, false, true, false, false, true]);
        let width = width_px(b, 2, 96);
        let at = |x| hit_at(b, x, width, 96, 2);
        assert_eq!(at(7), None);
        assert_eq!(at(8), Some(Hit::Button(Button::Start)));
        assert_eq!(at(8 + 44), Some(Hit::Button(Button::Search)));
        assert_eq!(at(8 + 88), None); // the separator
        assert_eq!(at(8 + 88 + 8), Some(Hit::Slot(0)));
        assert_eq!(at(8 + 88 + 8 + 56), Some(Hit::Slot(1)));
        assert_eq!(at(8 + 88 + 8 + 112), None); // the tail separator
        assert_eq!(at(width - STRIP_DIPS - 44), Some(Hit::Button(Button::Tray)));
        assert_eq!(at(width - STRIP_DIPS), Some(Hit::Button(Button::Desktop)));
        assert_eq!(at(width - 1), Some(Hit::Button(Button::Desktop)));
        // Without a strip the chevron ends one pad from the edge.
        let b = only(Button::Tray);
        let width = width_px(b, 1, 96);
        assert_eq!(
            hit_at(b, width - 8 - 1, width, 96, 1),
            Some(Hit::Button(Button::Tray))
        );
        assert_eq!(hit_at(b, width - 1, width, 96, 1), None);
        // A narrow work area clips slots, never the buttons at either end.
        let b = ALL;
        let narrow = width_px(b, 1, 96) + 56;
        assert_eq!(visible_slots(b, narrow, 96, 5), 2);
        assert_eq!(
            hit_at(b, narrow - 1, narrow, 96, 5),
            Some(Hit::Button(Button::Desktop))
        );
        assert_eq!(
            hit_at(b, 8, narrow, 96, 5),
            Some(Hit::Button(Button::Start))
        );
    }

    #[test]
    fn slot_centres_and_drops_follow_the_lead_buttons() {
        let b = Buttons::of([true, true, false, false, false, false, false]);
        // pad 8 + two buttons 88 + separator 8 = 104; slot 1 centre = 104 + 56 + 28.
        let width = width_px(b, 6, 96);
        assert_eq!(slot_center_x(1000, b, width, 1, 96), 1000 + 104 + 56 + 28);
        assert_eq!(slot_center_x(1000, N, 0, 0, 96), 1000 + 8 + 28);
        assert_eq!(drop_index(b, 104 + 56, width, 96, 6, 3), Some(1));
        assert_eq!(drop_index(b, 0, width, 96, 6, 3), Some(0));
    }

    #[test]
    fn keys_follow_the_buttons() {
        assert_eq!(Button::Start.keys(), Some(&[VK_LWIN][..]));
        assert_eq!(Button::Search.keys(), Some(&[VK_LWIN, VK_S][..]));
        assert_eq!(Button::TaskView.keys(), Some(&[VK_LWIN, VK_TAB][..]));
        assert_eq!(Button::Desktop.keys(), Some(&[VK_LWIN, VK_D][..]));
        assert_eq!(Button::Quick.keys(), None);
        assert_eq!(Button::Tray.keys(), None);
        let keys: Vec<_> = Button::ALL.iter().map(|b| b.key()).collect();
        assert_eq!(
            keys,
            [
                "dbstart", "dbsearch", "dbtask", "dbdesk", "dbquick", "dbplaces", "dbtray"
            ]
        );
    }

    fn mirrored(b: Buttons) -> Buttons {
        Buttons { rtl: true, ..b }
    }

    #[test]
    fn a_mirrored_dock_is_the_exact_mirror_of_the_left_to_right_one() {
        for dpi in [96, 144, 192] {
            for mask in 0..64u32 {
                let ltr = Buttons::of(std::array::from_fn(|i| mask >> i & 1 == 1));
                let rtl = mirrored(ltr);
                let slots = 4;
                let width = width_px(ltr, slots, dpi);
                assert_eq!(width_px(rtl, slots, dpi), width);
                for index in 0..cell_count(ltr, slots) {
                    let hit = cell(ltr, slots, index).unwrap();
                    let (left, w) = span(ltr, width, dpi, hit);
                    // The same cell sits at the opposite edge, and the first cell is now the rightmost.
                    assert_eq!(span(rtl, width, dpi, hit), (width - left - w, w));
                    assert_eq!(
                        hit_at(rtl, width - left - w / 2 - 1, width, dpi, slots),
                        Some(hit)
                    );
                }
                for x in 0..width {
                    assert_eq!(
                        hit_at(rtl, x, width, dpi, slots),
                        hit_at(ltr, width - 1 - x, width, dpi, slots),
                        "mask {mask} dpi {dpi} x {x}"
                    );
                    assert_eq!(
                        drop_index(rtl, x, width, dpi, slots, 3),
                        drop_index(ltr, width - 1 - x, width, dpi, slots, 3)
                    );
                }
                let flipped = separators(ltr, width, dpi).map(|x| x.map(|x| width - x));
                assert_eq!(separators(rtl, width, dpi), flipped);
                assert_eq!(
                    slot_center_x(0, rtl, width, 1, dpi),
                    width - slot_center_x(0, ltr, width, 1, dpi)
                );
            }
        }
    }

    #[test]
    fn separators_sit_between_the_button_groups_and_the_slots() {
        let width = width_px(Buttons::default(), 2, 96);
        // Lead: pad 8 + two buttons 88 = 96, then half of the 8 separator.
        let [lead, tail] = separators(Buttons::default(), width, 96);
        assert_eq!(lead, Some(8 + 88 + 2));
        // Tail: separator 8, hidden icons 44, quick settings 44, the desktop control at the right edge.
        assert_eq!(tail, Some(width - (8 + 44 + 44 + STRIP_DIPS) + 8));
        assert_eq!(separators(N, width, 96), [None, None]);
        assert_eq!(separators(only(Button::Start), width, 96)[1], None);
        assert_eq!(separators(only(Button::Desktop), width, 96)[0], None);
    }

    #[test]
    fn a_narrow_bar_hides_the_last_slots() {
        let b = Buttons::default();
        let narrow = width_px(b, 4, 96) - 2 * px(SLOT_DIPS, 96);
        assert_eq!(visible_slots(b, narrow, 96, 4), 2);
    }

    #[test]
    fn a_wide_bar_centres_its_slots_only_when_asked() {
        let b = Buttons::default();
        let wide = width_px(b, 3, 96) + 400;
        let left = span(b, wide, 96, Hit::Slot(0)).0;
        let centred = Buttons { centre: 3, ..b };
        let mid = span(centred, wide, 96, Hit::Slot(0)).0;
        assert_eq!(mid - left, 200);
        let (at, w) = span(centred, wide, 96, Hit::Slot(1));
        assert_eq!(hit_at(centred, at + w / 2, wide, 96, 3), Some(Hit::Slot(1)));
        // A dock exactly as wide as its slots does not move.
        let snug = width_px(b, 3, 96);
        assert_eq!(
            span(centred, snug, 96, Hit::Slot(0)).0,
            span(b, snug, 96, Hit::Slot(0)).0
        );
    }
}
