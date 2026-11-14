use crate::pins::{self, Pins};
use crate::sort;
use crate::window_model::{Identity, WindowKey, WindowModel, WindowRecord};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    Globalization::{
        GetLocaleInfoW, GetUserDefaultUILanguage, LOCALE_IREADINGLAYOUT, LOCALE_RETURN_NUMBER,
    },
    System::Threading::GetCurrentProcessId,
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
            VK_D, VK_LWIN, VK_S, VK_TAB,
        },
        Shell::{SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW, ShellExecuteExW},
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
            PostMessageW, SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL, SetForegroundWindow,
            ShowWindowAsync, WM_CLOSE,
        },
    },
};
use windows::core::{HSTRING, PCWSTR};

/// Dock geometry in DIPs. Slot 0 is the "all windows" picker; app groups follow.
pub const SLOT_DIPS: i32 = 56;
pub const PAD_DIPS: i32 = 8;
pub const ICON_DIPS: i32 = 40;
pub const HEIGHT_DIPS: i32 = 64;
/// The bar's corner radius (the floating bar's, and the attached dock's top corners).
pub const RADIUS_DIPS: i32 = 20;

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
            }),
        }
    }
    items
}

/// Pins first, in pin order (running group or a launcher with no windows), then unpinned groups in `items` order.
/// An `Executable` pin also claims a group whose AUMID identity has the same exe, so a pin survives that upgrade.
/// With no pins this equals `items(model, desktop_only)`. A pin with no window left on this desktop stays, as a launcher.
pub fn items_with_pins(model: &WindowModel, pins: &Pins, desktop_only: bool) -> Vec<DockItem> {
    let mut running: Vec<Option<DockItem>> =
        items(model, desktop_only).into_iter().map(Some).collect();
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
                }),
        );
    }
    merged.extend(running.into_iter().flatten());
    merged
}

/// Taskbar-style buttons beside the app slots (features D2). Start, Search and Task View sit left of the
/// picker; the minimized-apps chevron and the show-desktop strip sit at the right end.
pub const BTN_DIPS: i32 = 44;
pub const SEP_DIPS: i32 = 8;
pub const STRIP_DIPS: i32 = 26;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Start,
    Search,
    TaskView,
    Minimized,
    Desktop,
}
impl Button {
    pub const ALL: [Button; 5] = [
        Button::Start,
        Button::Search,
        Button::TaskView,
        Button::Minimized,
        Button::Desktop,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    /// The accessible name and the hover tooltip.
    pub fn label(self) -> &'static str {
        [
            "Start",
            "Search",
            "Task View",
            "Show minimized apps",
            "Show desktop",
        ][self.index()]
    }
    /// Settings store key.
    pub fn key(self) -> &'static str {
        ["dbstart", "dbsearch", "dbtask", "dbmin", "dbdesk"][self.index()]
    }
    /// The keys a click sends: the Windows key alone opens Start; the rest are shell shortcuts. None for the
    /// chevron, which opens ROQA's own list.
    pub fn keys(self) -> Option<&'static [VIRTUAL_KEY]> {
        match self {
            Button::Start => Some(&[VK_LWIN]),
            Button::Search => Some(&[VK_LWIN, VK_S]),
            Button::TaskView => Some(&[VK_LWIN, VK_TAB]),
            Button::Desktop => Some(&[VK_LWIN, VK_D]),
            Button::Minimized => None,
        }
    }
}

/// Which buttons are on, and the dock's reading direction. Default: Start, Search, the minimized list and show-desktop; Task View off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Buttons {
    pub shown: [bool; 5],
    /// Mirrored for a right-to-left UI language: the dock runs from the right edge, as the taskbar does.
    /// The host sets it from the system; it is not a stored setting.
    pub rtl: bool,
}
impl Buttons {
    #[cfg(test)]
    pub const NONE: Buttons = Buttons::of([false; 5]);
    pub const fn of(shown: [bool; 5]) -> Self {
        Self { shown, rtl: false }
    }
    pub fn on(self, button: Button) -> bool {
        self.shown[button.index()]
    }
    /// Start, Search and Task View, in that order.
    fn lead(self) -> impl Iterator<Item = Button> {
        [Button::Start, Button::Search, Button::TaskView]
            .into_iter()
            .filter(move |b| self.on(*b))
    }
    fn has_tail(self) -> bool {
        self.on(Button::Minimized) || self.on(Button::Desktop)
    }
    /// The chevron, then the strip.
    fn tail(self) -> impl Iterator<Item = Button> {
        [Button::Minimized, Button::Desktop]
            .into_iter()
            .filter(move |b| self.on(*b))
    }
}
impl Default for Buttons {
    fn default() -> Self {
        Buttons::of([true, true, false, true, true])
    }
}

/// What a point is: an app slot (0 is the picker) or a button.
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

/// Whether a catalog window belongs in the minimized-apps list: minimized, and on this desktop when the
/// dock is limited to it.
pub fn lists_minimized(window: &WindowRecord, desktop_only: bool) -> bool {
    window.minimized && (window.current_desktop || !desktop_only)
}

/// The most rows the minimized popup shows.
pub const MINIMIZED_MAX: usize = 12;

/// Minimized windows for the chevron, in title order (then key) so the list is stable.
pub fn minimized_windows(model: &WindowModel, desktop_only: bool) -> Vec<&WindowRecord> {
    let mut list: Vec<_> = model
        .windows
        .iter()
        .filter(|window| lists_minimized(window, desktop_only))
        .collect();
    sort::by(&mut list, |a, b| (&a.title, a.key).cmp(&(&b.title, b.key)));
    list
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
        + if buttons.on(Button::Minimized) {
            px(BTN_DIPS, dpi)
        } else {
            0
        }
        + px(
            if buttons.on(Button::Desktop) {
                STRIP_DIPS
            } else {
                PAD_DIPS
            },
            dpi,
        )
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
            lead_px(buttons, dpi) + px(SLOT_DIPS, dpi) * slot as i32,
            px(SLOT_DIPS, dpi),
        ),
        Hit::Button(Button::Desktop) => (width_px - strip, strip),
        Hit::Button(Button::Minimized) => {
            let end = if buttons.on(Button::Desktop) {
                STRIP_DIPS
            } else {
                PAD_DIPS
            };
            (width_px - px(end, dpi) - btn, btn)
        }
        Hit::Button(button) => {
            let at = buttons.lead().position(|b| b == button).unwrap_or(0) as i32;
            (px(PAD_DIPS, dpi) + at * btn, btn)
        }
    }
}

/// The centre of dock slot `slot` (slot 0 is the picker) for a dock `width_px` wide whose left edge is `dock_x`.
pub fn slot_center_x(dock_x: i32, buttons: Buttons, width_px: i32, slot: usize, dpi: u32) -> i32 {
    let (left, width) = span(buttons, width_px, dpi, Hit::Slot(slot));
    dock_x + left + width / 2
}

/// Slots that fit inside the realized window width; the rest stay reachable through the picker.
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
    let offset = x_px - lead_px(buttons, dpi);
    if offset < 0 {
        return None;
    }
    let slot = (offset / px(SLOT_DIPS, dpi).max(1)) as usize;
    (slot < visible_slots(buttons, width_px, dpi, slots)).then_some(Hit::Slot(slot))
}

/// Window x of the hairline between the lead buttons and the slots, and between the slots and the tail,
/// for the groups that exist.
pub fn separators(buttons: Buttons, width_px: i32, dpi: u32) -> [Option<i32>; 2] {
    let half = px(SEP_DIPS, dpi) / 2;
    let at = |x| if buttons.rtl { width_px - x } else { x };
    [
        (buttons.lead().count() > 0).then(|| at(lead_px(buttons, dpi) - half)),
        buttons
            .has_tail()
            .then(|| at(width_px - tail_px(buttons, dpi) + half)),
    ]
}

/// Pointer travel (DIPs) before a press on a pinned slot becomes a reorder drag instead of a click.
pub const DRAG_DIPS: i32 = 6;

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
        .saturating_sub(1)
        .min(pins)
        .checked_sub(1)?;
    let x_px = ltr_x(buttons, x_px, width_px);
    let slot = (x_px - lead_px(buttons, dpi)).div_euclid(px(SLOT_DIPS, dpi).max(1));
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
    launch_with(target, None)
}

/// `launch` with command-line `parameters`, for the file shelf's reveal.
pub fn launch_with(target: &str, parameters: Option<&str>) -> bool {
    let file = HSTRING::from(target);
    let parameters = parameters.map(HSTRING::from);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI,
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
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
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
    };
    let events: Vec<INPUT> = keys
        .iter()
        .map(|&vk| key(vk, false))
        .chain(keys.iter().rev().map(|&vk| key(vk, true)))
        .collect();
    // SAFETY: `events` is a live slice of fully initialized INPUT structs and the size is theirs.
    unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) as usize == events.len() }
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
    use crate::motion::{EASE_OUT, Tween};
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
        slide.retarget(1.0, super::SLIDE_TIME, EASE_OUT);
        // Past its end the tween sits exactly on 1 and reports no running frames, so nothing keeps ticking.
        let end = Instant::now() + super::SLIDE_TIME * 2;
        assert_eq!(slide.value(end), 1.0);
        assert!(!slide.running(end));
        assert_eq!(super::slide_offset(slide.value(end), 64), 0);
        // Leaving again starts from where it is, so an interruption never jumps.
        slide.retarget(0.0, super::SLIDE_TIME, EASE_OUT);
        assert!(slide.running(Instant::now()));
    }

    use super::*;
    use crate::window_model::WindowRecord;

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
                record(5, exe.clone(), true),
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
        // 96 dpi: pad 8, slot 56; slot 0 is the picker, so item i is slot i + 1 at x = 8 + 56 * (i + 1).
        let at = |x, slots, pins| drop_index(N, x, width_px(N, slots, 96), 96, slots, pins);
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
        assert_eq!(drop_index(N, 10_000, width_px(N, 3, 96), 96, 8, 4), Some(1));
        // Agrees with `slot_at`: the slot a click would hit is the slot a drop lands on.
        let width = width_px(N, 8, 96);
        for x in 0..width {
            if let Some(slot @ 1..=4) = slot_at(x, width, 96, 8) {
                assert_eq!(drop_index(N, x, width, 96, 8, 4), Some(slot - 1), "x {x}");
            }
        }
        // 150%: pad 12, slot 84.
        assert_eq!(
            drop_index(N, 12 + 84 * 2, width_px(N, 5, 144), 144, 5, 3),
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
        let mut on = [false; 5];
        on[button.index()] = true;
        Buttons::of(on)
    }
    const ALL: Buttons = Buttons::of([true; 5]);

    #[test]
    fn width_grows_by_each_button_and_none_keeps_the_old_dock() {
        let base = width_px(N, 3, 96);
        assert_eq!(base, PAD_DIPS * 2 + SLOT_DIPS * 3);
        // Lead buttons share one separator; the tail has one separator, the chevron and the strip.
        assert_eq!(
            width_px(only(Button::Start), 3, 96),
            base + BTN_DIPS + SEP_DIPS
        );
        assert_eq!(
            width_px(only(Button::TaskView), 3, 96),
            base + BTN_DIPS + SEP_DIPS
        );
        assert_eq!(
            width_px(only(Button::Minimized), 3, 96),
            base + SEP_DIPS + BTN_DIPS
        );
        assert_eq!(
            width_px(only(Button::Desktop), 3, 96),
            base - PAD_DIPS + SEP_DIPS + STRIP_DIPS
        );
        assert_eq!(
            width_px(ALL, 3, 96),
            PAD_DIPS + 3 * BTN_DIPS + SEP_DIPS + SLOT_DIPS * 3 + SEP_DIPS + BTN_DIPS + STRIP_DIPS
        );
        assert_eq!(Buttons::default().shown, [true, true, false, true, true]);
    }

    #[test]
    fn cells_never_overlap_and_tile_the_dock_at_any_scale() {
        for dpi in [96, 120, 144, 192] {
            for mask in 0..32u32 {
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
        // 96 dpi, Start + Search + chevron + strip, 2 slots: pad 8, buttons 44, sep 8, slots 56.
        let b = Buttons::of([true, true, false, true, true]);
        let width = width_px(b, 2, 96);
        let at = |x| hit_at(b, x, width, 96, 2);
        assert_eq!(at(7), None);
        assert_eq!(at(8), Some(Hit::Button(Button::Start)));
        assert_eq!(at(8 + 44), Some(Hit::Button(Button::Search)));
        assert_eq!(at(8 + 88), None); // the separator
        assert_eq!(at(8 + 88 + 8), Some(Hit::Slot(0)));
        assert_eq!(at(8 + 88 + 8 + 56), Some(Hit::Slot(1)));
        assert_eq!(at(8 + 88 + 8 + 112), None); // the tail separator
        assert_eq!(
            at(width - STRIP_DIPS - 44),
            Some(Hit::Button(Button::Minimized))
        );
        assert_eq!(at(width - STRIP_DIPS), Some(Hit::Button(Button::Desktop)));
        assert_eq!(at(width - 1), Some(Hit::Button(Button::Desktop)));
        // Without a strip the chevron ends one pad from the edge.
        let b = only(Button::Minimized);
        let width = width_px(b, 1, 96);
        assert_eq!(
            hit_at(b, width - 8 - 1, width, 96, 1),
            Some(Hit::Button(Button::Minimized))
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
        let b = Buttons::of([true, true, false, false, false]);
        // pad 8 + two buttons 88 + separator 8 = 104; slot 1 centre = 104 + 56 + 28.
        let width = width_px(b, 6, 96);
        assert_eq!(slot_center_x(1000, b, width, 1, 96), 1000 + 104 + 56 + 28);
        assert_eq!(slot_center_x(1000, N, 0, 0, 96), 1000 + 8 + 28);
        assert_eq!(drop_index(b, 104 + 56 * 2, width, 96, 6, 3), Some(1));
        assert_eq!(drop_index(b, 0, width, 96, 6, 3), Some(0));
    }

    #[test]
    fn keys_follow_the_buttons() {
        assert_eq!(Button::Start.keys(), Some(&[VK_LWIN][..]));
        assert_eq!(Button::Search.keys(), Some(&[VK_LWIN, VK_S][..]));
        assert_eq!(Button::TaskView.keys(), Some(&[VK_LWIN, VK_TAB][..]));
        assert_eq!(Button::Desktop.keys(), Some(&[VK_LWIN, VK_D][..]));
        assert_eq!(Button::Minimized.keys(), None);
        let keys: Vec<_> = Button::ALL.iter().map(|b| b.key()).collect();
        assert_eq!(keys, ["dbstart", "dbsearch", "dbtask", "dbmin", "dbdesk"]);
    }

    #[test]
    fn minimized_filter_keeps_minimized_windows_on_this_desktop() {
        let mut model = WindowModel::default();
        for (handle, title, minimized, here) in [
            (1, "zeta", true, true),
            (2, "alpha", true, true),
            (3, "open", false, true),
            (4, "away", true, false),
        ] {
            let mut w = record(handle, Identity::Unavailable(handle as u32), false);
            (w.title, w.minimized, w.current_desktop) = (title.into(), minimized, here);
            model.windows.push(w);
        }
        let titles = |desktop_only| -> Vec<_> {
            minimized_windows(&model, desktop_only)
                .iter()
                .map(|w| w.title.clone())
                .collect()
        };
        assert_eq!(titles(false), ["alpha", "away", "zeta"]);
        assert_eq!(titles(true), ["alpha", "zeta"]);
        assert!(minimized_windows(&WindowModel::default(), false).is_empty());
    }

    fn mirrored(b: Buttons) -> Buttons {
        Buttons { rtl: true, ..b }
    }

    #[test]
    fn a_mirrored_dock_is_the_exact_mirror_of_the_left_to_right_one() {
        for dpi in [96, 144, 192] {
            for mask in 0..32u32 {
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
        assert_eq!(lead, Some(8 + 88 + 4));
        // Tail: separator 8, chevron 44, the desktop control at the right edge.
        assert_eq!(tail, Some(width - (8 + 44 + STRIP_DIPS) + 4));
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
    fn every_button_has_a_distinct_tooltip_label() {
        let labels: Vec<_> = Button::ALL.iter().map(|b| b.label()).collect();
        assert!(labels.iter().all(|label| !label.is_empty()));
        assert!(
            labels
                .iter()
                .enumerate()
                .all(|(at, label)| !labels[..at].contains(label))
        );
    }
}
