//! The settings window: a sidebar of pages and a card pane, built from the `PAGES` table (one loop creates the controls, one lays them out, `commit`/`show` read and write them by id).
//! Standard child controls give mouse use and text scaling (screen readers are deliberately not supported; keyboard use is `keyboard.rs`); only the background, cards and sidebar are drawn here, in GDI with stock objects.
//! It owns no timer or thread: it exists only while open and does work only in the message loop turns its own controls cause (and, while a wheel glide runs, the host's frames: `scroll.rs`).
//! A change goes to the host (`take_change`) which applies it live; a timer request likewise (`take_start`).

use crate::{
    agent_hooks,
    dialog::{
        ACTION, CLOSE, DIALOG_STYLE, RELAYOUT, SCROLL, WHEEL, control, dialog_proc, empty_dialog,
        wide,
    },
    dock::{foreground_key, restore_foreground},
    graphics::{GdiBatch, Glyph, batch_gdi},
    hotkeys,
    layout::{Bounds, DockStyle},
    motion,
    picker_font::PickerFont,
    pomo,
    prayer::Method,
    prayer_os,
    prayer_tab::{self, LEADS, SILENT},
    settings::{MAX_OFF, MAX_PATH, MAX_SHIFT, NOTCH_SIZES, Settings, parse_timer},
    theme::Theme,
    topology::Monitor,
    water,
    weather::{MAX_CITY, Units},
    window_model::WindowKey,
};
use keyboard::{focus_ring, ringed, ringed_in};
use std::cell::{Cell, RefCell};
use windows::{
    Win32::{
        Foundation::{COLORREF, HANDLE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::{
            Dwm::{
                DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
                DwmSetWindowAttribute,
            },
            Gdi::*,
        },
        System::SystemInformation::GetTickCount64,
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            Controls::*,
            HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
            Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus},
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, PCWSTR, Result, w},
};

const PIN: usize = 140;
const EARLIER: usize = 141;
const LATER: usize = 142;
const DOCK: usize = 100;
const NOTCH: usize = 101;
const SOUND: usize = 102;
const SHELF: usize = 103;
const DIAGNOSTICS: usize = 104;
const TIMER: usize = 105;
const START: usize = 106;
const STATUS: usize = 107;
const RESET: usize = 108;
const LABEL: usize = 109;
const THEME: usize = 110;
const DESKTOP: usize = 111;
const EDGE_VOLUME: usize = 112;
const EDGE_BRIGHTNESS: usize = 113;
const CITY: usize = 114;
const DETECT: usize = 115;
const UNITS: usize = 116;
const DOCKSTYLE: usize = 117;
/// Prayer page: the moon as a dock button.
const PMOON: usize = 118;
const CAPTION: usize = 119;
const MONITOR: usize = 120;
/// The "Displays" card (index in page 0): one switch row per display.
const DISPLAYS_CARD: usize = 1;
/// One checkbox per dock button (`dock::Button::ALL` order).
const DBTN: usize = 130;
/// One checkbox per notch tab (`Tab::ALL` order), then the mascot eyes and the privacy dots.
const TAB: usize = 150;
const EYES: usize = TAB + 7;
const PRIVACY: usize = TAB + 8;
const PRAYER: usize = 160;
const PMETHOD: usize = 161;
const PSOUND: usize = 162;
const PBROWSE: usize = 163;
const PTEST: usize = 164;
const PLEAD: usize = 165;
const LIMCODEX: usize = 166;
const LIMAGY: usize = 167;
const LIMGROUP: usize = 168;
const LIMSNIP: usize = 169;
/// Five prayer fine-tune edits, the notch size, open and animation lists, then the full-screen, sound, pause switches.
const PSHIFT: usize = 170;
const NSIZE: usize = 175;
const NOPEN: usize = 176;
const ANIM: usize = 177;
const FSHIDE: usize = 178;
const RSOUNDS: usize = 179;
const PPAUSE: usize = 180;
/// Water reminders: the switch, the five numbers (`water::Config` order) and the "I drank" button.
const WATER: usize = 181;
const WCFG: usize = 182;
const WDRANK: usize = 187;
/// Pomodoro: the four numbers (`pomo::Config` order), start or pause, skip.
const PCFG: usize = 188;
const PSTART: usize = 192;
const PSKIP: usize = 193;
/// The Limits page's "Copy" button for the statusline command.
const LIMCOPY: usize = 194;
const WIDGET_LIMIT: usize = 310;
const WIDGET_FIRST: usize = 311;
const MATCH_ACCENT: usize = 199;
const DOCK_BADGES: usize = 202;
const CLIP: usize = 203;
const DOCK_HIDE: usize = 204;
const NOTCH_HIDE: usize = 205;
const AUTOSTART: usize = 234;
const HIDE_CAPTURE: usize = 235;
const DOCK_LEFT: usize = 206;
/// The reveal and hide delay lists.
const REVEAL: usize = 232;
const HIDE: usize = 233;
/// The Widgets card's extra rows: the two lower slots, network units, the disk, the low-battery level, the
/// week's first day and the closed pill's low-battery bolt.
const NETU: usize = 209;
const DISKD: usize = 210;
const LOWAT: usize = 211;
const WEEK1: usize = 212;
const PILLLOW: usize = 213;
/// The agent-status switch, then one button per `agent_hooks::AGENTS` entry.
const AGENT_ON: usize = 214;
const AGENT_BTN: usize = 215;
/// The row after them: the Claude Code permission questions (the last `agent_hooks::AGENTS` entry).
const ASK_BTN: usize = 236;
/// The Windows notifications peek.
const NOTIFS: usize = 237;
const CAPS_PILL: usize = 238;
const HIDE_TASKBAR: usize = 239;
/// "Add all" in the Agent status card: every detected agent that has no hook yet.
const ADD_ALL: usize = 240;
/// The five shortcut switches (`hotkeys::KEYS` order).
const HOTKEY: usize = 222;
/// The hidden-apps list and its "Show in dock" button.
const HIDDEN: usize = 230;
const SHOW: usize = 231;
const SIDE: usize = 98;
/// The compact layout's category picker, and the local tabs (one radio button per page, `TABBTN + page`).
const NAV: usize = 97;
const TABBTN: usize = 330;
const OK: usize = 1;
/// `SS_NOTIFY`: the footer message takes clicks, so scrolled rows cannot be hit under it.
const SS_NOTIFY: u32 = 0x100;
/// Timer length shown when the window opens.
const FIRST_TIMER: &str = "5";
/// The footer's line when nothing needs saying.
const DEFAULT_STATUS: &str = "Changes apply as you go.";
/// DIPs: the window's preferred client size.
const DESIGN_W: i32 = 880;
const DESIGN_H: i32 = 680;

/// What the Settings window asks the host to do with the prayer sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundAction {
    Browse,
    Test,
}

struct Item {
    id: usize,
    kind: Kind,
    /// The page it is on (an index into `PAGES`).
    page: u8,
    card: u8,
    /// The control, its label and its caption: a null handle is absent.
    h: HWND,
    label: HWND,
    cap: HWND,
    /// The row's top and bottom in its page, as `place` last put it (for focus to reveal the whole row).
    row: Cell<(i32, i32)>,
}

pub struct SettingsWindow {
    window: HWND,
    /// The rail (a list of destinations), the compact layout's picker in its place, the clipped viewport, one
    /// content window per page (the parent of that page's controls) and one local tab button per page.
    side: HWND,
    nav: HWND,
    search: search::Search,
    viewport: HWND,
    contents: Vec<HWND>,
    tabs: Vec<HWND>,
    status: HWND,
    close: HWND,
    items: Vec<Item>,
    /// The page shown (an index into `PAGES`), its destination, and the page last shown on each destination.
    page: usize,
    route: usize,
    last_page: [usize; ROUTES.len()],
    /// The scroll position each page was left at, for this window's life only (nothing is stored).
    offsets: Vec<i32>,
    frame: view::Frame,
    move_pin: Option<(usize, bool)>,
    show_hidden: Option<usize>,
    /// The work area it opens in: the monitor ROQA is used on.
    work: Bounds,
    /// The connected monitors' GDI devices (at most `MAX_OFF`); monitor `n` is the checkbox `MONITOR + n`.
    devices: Vec<String>,
    /// The same monitors with their size and place.
    monitors: Vec<Monitor>,
    /// The audio files found (and the chosen one if it is elsewhere), full paths.
    sounds: Vec<String>,
    sound_action: Option<SoundAction>,
    font: Option<PickerFont>,
    pal: Pal,
    factor: f64,
    scroll: i32,
    scroll_max: i32,
    /// Carried sub-pixel remainder for fine wheel / touchpad deltas (in 120ths of a pixel).
    wheel_remainder: i32,
    /// What the host last got; a change is a difference from this.
    current: Settings,
    change: Option<Settings>,
    start: Option<u64>,
    drank: bool,
    /// A Pomodoro request: false start or pause, true skip.
    pomo: Option<bool>,
    return_to: Option<WindowKey>,
    closed: bool,
}

/// Colours as 0xRRGGBB.
#[derive(Clone, Copy)]
struct Pal {
    side: u32,
    content: u32,
    card: u32,
    line: u32,
    /// The 1 px frame round an edit or a drop-down: a step above the card, never the theme's bright border.
    field: u32,
    /// What an edit's box is filled with (the window colour in high contrast, where `sel` is the highlight).
    well: u32,
    sel: u32,
    text: u32,
    dim: u32,
    accent: u32,
    /// Toggle track when off; the toggle's knob (white in every theme, the window colour in high contrast).
    off: u32,
    thumb: u32,
    /// Ink on an accent capsule (a toggle's thumb when on, a primary button's text).
    on_accent: u32,
    seltext: u32,
    dark: bool,
    contrast: bool,
}

/// What the window's own erase handlers draw: the fixed frame on the dialog, the page on its content window.
struct Paint {
    pal: Pal,
    scale: f64,
    font: HFONT,
    title_font: HFONT,
    /// The body font in semibold (group headings, the name beside the eyes).
    bold: HFONT,
    display_meta: HFONT,
    display_number: HFONT,
    /// The destination's name and caption, under the eyes' rail.
    title: &'static str,
    caption: &'static str,
    frame: view::Frame,
    /// The page's headings, panels and field boxes, in its content window's coordinates.
    geometry: Geometry,
    results: Option<search::Results>,
    /// The scroll bar in the right gutter, painted with the frame.
    bar: scroll::Bar,
}
thread_local! {
    static PAINT: RefCell<Option<Paint>> = const { RefCell::new(None) };
}

/// RGB and COLORREF (BGR) swap into each other.
fn swap(value: u32) -> u32 {
    (value & 0xff) << 16 | value & 0xff00 | value >> 16 & 0xff
}
fn rgb(value: u32) -> COLORREF {
    COLORREF(swap(value))
}
fn system(index: SYS_COLOR_INDEX) -> u32 {
    // SAFETY: scalar query.
    swap(unsafe { GetSysColor(index) })
}
fn high_contrast() -> bool {
    let mut hc = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    // SAFETY: fills a local struct of the size it declares.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            Some((&mut hc as *mut HIGHCONTRASTW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_ok() && hc.dwFlags & HCF_HIGHCONTRASTON == HCF_HIGHCONTRASTON
}

/// Settings consumes the active ROQA surface, ink and accent tokens.
fn palette(settings: &Settings) -> Pal {
    if high_contrast() {
        let (window, text, select) = (
            system(COLOR_WINDOW),
            system(COLOR_WINDOWTEXT),
            system(COLOR_HIGHLIGHT),
        );
        return Pal {
            side: window,
            content: window,
            card: window,
            line: text,
            field: text,
            well: window,
            sel: select,
            text,
            dim: system(COLOR_GRAYTEXT),
            accent: select,
            off: system(COLOR_GRAYTEXT),
            thumb: window,
            on_accent: system(COLOR_HIGHLIGHTTEXT),
            seltext: system(COLOR_HIGHLIGHTTEXT),
            dark: false,
            contrast: true,
        };
    }
    let tokens = crate::theme::system_palette(
        settings.theme,
        settings
            .match_accent
            .then(crate::theme::windows_accent_color)
            .flatten(),
    );
    let pack =
        |(r, g, b): crate::theme::Rgb| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
    let opaque = |crate::theme::Rgba(r, g, b, _)| pack((r, g, b));
    let body = opaque(tokens.body);
    let mix = |colour: crate::theme::Rgba| {
        let crate::theme::Rgba(r, g, b, alpha) = colour;
        let channel = |v: u8, shift: u32| {
            ((f32::from(v) * alpha + ((body >> shift) & 255u32) as f32 * (1.0 - alpha)).round()
                as u32)
                << shift
        };
        channel(r, 16) | channel(g, 8) | channel(b, 0)
    };
    let dark = settings.theme != Theme::Light;
    let tile = mix(tokens.tile);
    Pal {
        side: opaque(tokens.dock_hover),
        content: body,
        card: mix(crate::theme::Rgba(
            tokens.ink.0,
            tokens.ink.1,
            tokens.ink.2,
            0.08,
        )),
        line: tile,
        field: tile,
        well: tile,
        sel: tile,
        text: pack(tokens.ink),
        dim: pack(tokens.glyph),
        accent: pack(tokens.accent),
        off: tile,
        thumb: 0xffffff,
        on_accent: if dark { body } else { 0xffffff },
        seltext: pack(tokens.ink),
        dark,
        contrast: false,
    }
}

/// A capsule or card in one fill and one outline colour (the same colour means no outline).
unsafe fn round(dc: HDC, r: RECT, radius: i32, fill: u32, line: u32) {
    // Anti-aliased by Direct2D on this DC (the colours are COLORREFs).
    batch_gdi(dc, r, |b| b.round(r, radius, swap(fill), swap(line)));
}

unsafe fn fill(dc: HDC, r: RECT, colour: u32) {
    // SAFETY: stock brush on the caller's live DC.
    unsafe {
        SetDCBrushColor(dc, rgb(colour));
        FillRect(dc, &r, HBRUSH(GetStockObject(DC_BRUSH).0));
    }
}

unsafe fn write(dc: HDC, text: &str, mut r: RECT, colour: u32, flags: DRAW_TEXT_FORMAT) {
    // An empty buffer's pointer dangles, and DrawTextW reads it under DT_END_ELLIPSIS (a crash).
    if text.is_empty() {
        return;
    }
    let mut text: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: draws a local buffer into the caller's live DC.
    unsafe {
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, rgb(colour));
        // A mirrored window (right-to-left UI) reads its text right to left too.
        let reading = if GetLayout(dc) & LAYOUT_RTL.0 != 0 {
            DT_RTLREADING
        } else {
            DRAW_TEXT_FORMAT(0)
        };
        DrawTextW(
            dc,
            &mut text,
            &mut r,
            flags | reading | DT_SINGLELINE | DT_VCENTER,
        );
    }
}

use draw::{combo_proc, settings_proc};
use paint::paint;
use place::Geometry;
use routes::*;

/// A combo row kept inside `len` items; `CB_ERR` (no selection) becomes the first row.
fn clamp_selection(row: isize, len: usize) -> usize {
    (row.max(0) as usize).min(len.saturating_sub(1))
}
fn add_row(combo: HWND, text: &str) {
    let text = wide(text);
    // SAFETY: the combo box copies the terminated string synchronously.
    unsafe {
        SendMessageW(
            combo,
            CB_ADDSTRING,
            None,
            Some(LPARAM(text.as_ptr() as isize)),
        );
    }
}
fn select(combo: HWND, row: Option<usize>) {
    // SAFETY: scalar selection index on our own combo box.
    unsafe {
        SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(row.unwrap_or(0))), None);
    }
}
/// The selected row of a drop-down list; `None` when nothing is chosen.
/// The local date and time, for the name of an agent file's backup copy.
fn backup_stamp() -> String {
    let now = prayer_os::local_now();
    let (y, m, d) = now.ymd;
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        now.secs / 3600,
        now.secs / 60 % 60,
        now.secs % 60
    )
}

fn chosen_row(combo: HWND) -> Option<usize> {
    // SAFETY: scalar query of our own combo box.
    usize::try_from(unsafe { SendMessageW(combo, CB_GETCURSEL, None, None) }.0).ok()
}
fn checked(control: HWND) -> bool {
    // SAFETY: scalar query of our own checkbox.
    unsafe { SendMessageW(control, BM_GETCHECK, None, None) }.0 == 1
}
fn set_checked(control: HWND, on: bool) {
    // SAFETY: scalar message to our own checkbox.
    unsafe {
        SendMessageW(control, BM_SETCHECK, Some(WPARAM(usize::from(on))), None);
    }
}
fn text(control: HWND, capacity: usize) -> String {
    let mut buffer = vec![0u16; capacity + 1];
    // SAFETY: our own edit control; Windows writes at most the provided slice capacity.
    let length = unsafe { GetWindowTextW(control, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..length])
}
fn set_text(control: HWND, value: &str) -> Result<()> {
    let value = wide(value);
    // SAFETY: the control synchronously copies the terminated string.
    unsafe { SetWindowTextW(control, PCWSTR(value.as_ptr())) }
}

/// The boolean setting a checkbox id stands for (monitors are not one: they are per device).
fn flag(s: &mut Settings, id: usize) -> Option<&mut bool> {
    Some(match id {
        DOCK => &mut s.dock,
        NOTCH => &mut s.notch,
        SOUND => &mut s.timer_sound,
        DESKTOP => &mut s.desktop_only,
        EDGE_VOLUME => &mut s.edge_volume,
        EDGE_BRIGHTNESS => &mut s.edge_brightness,
        EYES => &mut s.eyes,
        PRIVACY => &mut s.privacy,
        CLIP => &mut s.clipboard,
        NOTIFS => &mut s.notifications,
        DETECT => &mut s.detect,
        PRAYER => &mut s.prayer_remind,
        LIMCODEX => &mut s.limits_codex,
        LIMAGY => &mut s.limits_agy,
        FSHIDE => &mut s.hide_fullscreen,
        AUTOSTART => &mut s.autostart,
        HIDE_CAPTURE => &mut s.hide_capture,
        CAPS_PILL => &mut s.caps_pill,
        HIDE_TASKBAR => &mut s.hide_taskbar,
        RSOUNDS => &mut s.reminder_sounds,
        PPAUSE => &mut s.prayer_pause,
        PMOON => &mut s.dock_moon,
        WATER => &mut s.water,
        MATCH_ACCENT => &mut s.match_accent,
        DOCK_BADGES => &mut s.dock_badges,
        DOCK_HIDE => &mut s.dock_hide,
        NOTCH_HIDE => &mut s.notch_hide,
        DOCK_LEFT => &mut s.dock_left,
        311..=316 => &mut s.dock_buttons.widgets[id - WIDGET_FIRST],
        PILLLOW => &mut s.widgets.pill_low,
        AGENT_ON => &mut s.agent_status,
        HOTKEY..=226 => &mut s.hotkeys[id - HOTKEY],
        TAB..=159 if id - TAB < 6 => &mut s.tabs[id - TAB],
        DBTN..=136 => &mut s.dock_buttons.shown[id - DBTN],
        _ => return None,
    })
}
/// The row of `all` that `now` is (the first row when it is not listed).
fn row_of<T: PartialEq>(all: &[T], now: &T) -> usize {
    all.iter().position(|x| x == now).unwrap_or(0)
}
/// Sets `slot` to the row `row` of `all`; nothing chosen or out of range keeps it.
fn pick<T: Copy>(all: &[T], slot: &mut T, row: Option<usize>) {
    if let Some(v) = row.and_then(|i| all.get(i)) {
        *slot = *v;
    }
}
/// The heads-up list, one label per `LEADS` entry (0, 5, 10, 15).
const LEAD_ROWS: [&str; 4] = [
    "Off",
    "5 minutes before",
    "10 minutes before",
    "15 minutes before",
];
fn disk_row(drive: Option<u8>) -> usize {
    drive
        .and_then(|d| crate::widgets::fixed_drives().iter().position(|x| *x == d))
        .map_or(0, |i| i + 1)
}
fn disk_pick(slot: &mut Option<u8>, row: Option<usize>) {
    if let Some(row) = row {
        *slot = row
            .checked_sub(1)
            .and_then(|i| crate::widgets::fixed_drives().get(i).copied());
    }
}
/// The "Low battery at" percents; 0 leaves it to Windows.
const LOW_ROWS: [u8; 4] = [0, 10, 20, 30];
/// Adds the rows of the drop-down list that table row `id` names (the pin and sound lists are filled elsewhere).
fn fill_combo(h: HWND, id: usize) {
    let rows: &[&str] = match id {
        THEME => return Theme::ALL.iter().for_each(|t| add_row(h, t.label())),
        DOCKSTYLE => return DockStyle::ALL.iter().for_each(|s| add_row(h, s.label())),
        UNITS => return Units::ALL.iter().for_each(|u| add_row(h, u.label())),
        PMETHOD => return Method::ALL.iter().for_each(|m| add_row(h, m.label())),
        PLEAD => &LEAD_ROWS,
        LIMGROUP => &["Gemini models", "Claude and GPT models"],
        WIDGET_LIMIT => &["1", "2", "3", "4", "5", "6"],
        NETU => &["Bytes per second", "Bits per second"],
        WEEK1 => &["Monday", "Sunday"],
        LOWAT => &["Windows decides", "10%", "20%", "30%"],
        DISKD => {
            add_row(h, "System drive");
            return crate::widgets::fixed_drives()
                .iter()
                .for_each(|d| add_row(h, &format!("Drive {}", *d as char)));
        }
        NSIZE => &["Auto", "115%", "130%", "150%"],
        NOPEN => &["Hover", "Click"],
        ANIM => &["Normal", "Fast"],
        REVEAL | HIDE => &["Short", "Medium", "Long"],
        _ => &[],
    };
    rows.iter().for_each(|row| add_row(h, row));
}

impl SettingsWindow {
    pub fn create(
        current: &Settings,
        monitors: &[Monitor],
        factor: f64,
        work: Bounds,
    ) -> Result<Self> {
        let this = Self::open(current, monitors, factor, work)?;
        // SAFETY: explicit user request only. Windows retains its normal foreground restrictions.
        unsafe {
            let _ = ShowWindow(this.window, SW_SHOWNOACTIVATE);
            let _ = SetForegroundWindow(this.window);
        }
        Ok(this)
    }

    /// Builds the window, its controls and their values, and leaves it hidden.
    fn open(current: &Settings, monitors: &[Monitor], factor: f64, work: Bounds) -> Result<Self> {
        // Composited: the dialog and its children paint into one back buffer shown whole, never half drawn.
        let window = empty_dialog(480, 300, Some(settings_proc), WS_EX_COMPOSITED)?;
        if crate::dock::system_rtl() {
            // Children created after this inherit the mirrored layout.
            // SAFETY: our own dialog, before any child exists.
            unsafe {
                let ex = GetWindowLongPtrW(window, GWL_EXSTYLE);
                SetWindowLongPtrW(window, GWL_EXSTYLE, ex | WS_EX_LAYOUTRTL.0 as isize);
            }
        }
        let none = HWND::default();
        let mut this = Self {
            window,
            side: none,
            nav: none,
            search: search::Search::default(),
            viewport: none,
            contents: Vec::new(),
            tabs: Vec::new(),
            status: none,
            close: none,
            items: Vec::new(),
            page: 0,
            route: 0,
            last_page: std::array::from_fn(|route| pages_of(route).start),
            offsets: vec![0; PAGES.len()],
            frame: view::Frame::default(),
            move_pin: None,
            show_hidden: None,
            devices: Vec::new(),
            monitors: Vec::new(),
            sounds: prayer_os::discover(),
            sound_action: None,
            font: None,
            pal: palette(current),
            factor,
            scroll: 0,
            scroll_max: 0,
            wheel_remainder: 0,
            current: current.clone(),
            change: None,
            start: None,
            drank: false,
            pomo: None,
            return_to: foreground_key(),
            closed: false,
            work,
        };
        this.set_monitors(monitors);
        this.build()?;
        this.fill_sounds(&current.prayer_sound);
        this.show(current);
        this.relayout()?;
        // SAFETY: our own dialog.
        unsafe { SetWindowTextW(window, w!("ROQA settings"))? };
        Ok(this)
    }

    fn set_monitors(&mut self, monitors: &[Monitor]) {
        self.monitors = monitors.iter().take(MAX_OFF).cloned().collect();
        self.devices = self.monitors.iter().map(|m| m.device.clone()).collect();
    }

    fn h(&self, id: usize) -> HWND {
        self.items
            .iter()
            .find(|i| i.id == id)
            .map_or(HWND::default(), |i| i.h)
    }

    /// Each shortcut row's caption: its keys, or which of them another app already holds.
    pub fn set_hotkeys(&mut self, notes: &[String; hotkeys::COUNT]) {
        for (i, note) in notes.iter().enumerate() {
            if let Some(item) = self.items.iter().find(|item| item.id == HOTKEY + i) {
                let _ = set_text(item.cap, note);
            }
        }
        let _ = self.layout();
    }

    /// Each agent's row: whether it is installed and whether ROQA's hook is in its file.
    fn refresh_agents(&self) {
        let home = agent_hooks::home().unwrap_or_default();
        if let Some(all) = self.items.iter().find(|item| item.id == ADD_ALL) {
            let any = agent_hooks::AGENTS
                .iter()
                .any(|spec| agent_hooks::status(spec, &home) == agent_hooks::Status::Ready);
            // SAFETY: our own live child control.
            let _ = unsafe { EnableWindow(all.h, any) };
        }
        for (i, spec) in agent_hooks::AGENTS.iter().enumerate() {
            let id = if i < 7 { AGENT_BTN + i } else { ASK_BTN };
            let Some(item) = self.items.iter().find(|item| item.id == id) else {
                continue;
            };
            let status = agent_hooks::status(spec, &home);
            let (words, button) = match status {
                agent_hooks::Status::NotInstalled => ("not installed", "Add hook"),
                agent_hooks::Status::Ready => ("detected", "Add hook"),
                agent_hooks::Status::Added => ("hook added", "Undo"),
            };
            let _ = set_text(item.label, &format!("{}: {words}", spec.label));
            let _ = set_text(item.h, button);
            // SAFETY: our own live child control.
            let _ = unsafe { EnableWindow(item.h, status != agent_hooks::Status::NotInstalled) };
        }
    }

    /// "Add hook" or "Undo" for agent `i`: shows exactly what will change, and writes only on OK.
    fn agent_click(&self, i: usize) {
        let spec = &agent_hooks::AGENTS[i];
        let Some(home) = agent_hooks::home() else {
            return;
        };
        let add = agent_hooks::status(spec, &home) != agent_hooks::Status::Added;
        let make = || {
            std::env::current_exe()
                .map_err(|e| e.to_string())
                .and_then(|exe| agent_hooks::plan(spec, &home, &exe, add))
        };
        let plan = match make() {
            Ok(plan) => plan,
            Err(message) => return self.notify(&message, MB_ICONINFORMATION),
        };
        let body = if add {
            format!(
                "ROQA adds this to {}:

{}

A copy of the file is kept next to it.",
                plan.path.display(),
                plan.shown
            )
        } else {
            format!(
                "{}

Other hooks stay. A copy of the file is kept.",
                plan.shown
            )
        };
        if self.ask(spec.label, &body) {
            let stamp = backup_stamp();
            // The file may have changed while the box was open: write only the change the user was shown.
            let result = match make() {
                Ok(again) if plan.same_as(&again) => agent_hooks::apply(&plan, &stamp),
                _ => Err("The file changed. Try again.".to_owned()),
            };
            match result {
                Ok(()) if spec.id == "codex" && add => self.notify(
                    "Added. Trust it once in Codex with /hooks.",
                    MB_ICONINFORMATION,
                ),
                Ok(()) => {}
                Err(message) => self.notify(&message, MB_ICONWARNING),
            }
        }
        self.refresh_agents();
    }

    /// "Add all": one confirmation, then the hook of every detected agent that has none. Each is planned right
    /// before it is written, so agents sharing a file (Claude Code and its questions) build on each other.
    fn agent_add_all(&self) {
        let (Some(home), Ok(exe)) = (agent_hooks::home(), std::env::current_exe()) else {
            return;
        };
        let todo: Vec<_> = agent_hooks::AGENTS
            .iter()
            .filter(|spec| agent_hooks::status(spec, &home) == agent_hooks::Status::Ready)
            .collect();
        let labels: Vec<_> = todo.iter().map(|spec| spec.label).collect();
        let body = format!(
            "ROQA adds its hook to:\n\n{}\n\nA copy of each file is kept next to it.",
            labels.join("\n")
        );
        if todo.is_empty() || !self.ask("Add all hooks", &body) {
            return;
        }
        let stamp = backup_stamp();
        let mut problems = Vec::new();
        for spec in &todo {
            let result = agent_hooks::plan(spec, &home, &exe, true)
                .and_then(|plan| agent_hooks::apply(&plan, &stamp));
            if let Err(message) = result {
                problems.push(format!("{}: {message}", spec.label));
            }
        }
        if !problems.is_empty() {
            self.notify(&problems.join("\n"), MB_ICONWARNING);
        } else if todo.iter().any(|spec| spec.id == "codex") {
            self.notify(
                "Added. Trust it once in Codex with /hooks.",
                MB_ICONINFORMATION,
            );
        }
        self.refresh_agents();
    }

    fn ask(&self, title: &str, body: &str) -> bool {
        let (body, title) = (wide(body), wide(title));
        // SAFETY: the live settings dialog owns this confirmation; both strings are terminated.
        let answer = unsafe {
            MessageBoxW(
                Some(self.window),
                PCWSTR(body.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OKCANCEL | MB_DEFBUTTON2 | MB_ICONQUESTION,
            )
        };
        answer == IDOK
    }
    fn notify(&self, body: &str, icon: MESSAGEBOX_STYLE) {
        let body = wide(body);
        // SAFETY: the live settings dialog owns this notice; the string is terminated.
        unsafe {
            MessageBoxW(
                Some(self.window),
                PCWSTR(body.as_ptr()),
                w!("ROQA"),
                MB_OK | icon,
            );
        }
    }

    pub fn set_pins(&self, names: &[String], selected: usize) {
        let pins = self.h(PIN);
        // SAFETY: synchronous string copies to our own native combo box.
        unsafe {
            SendMessageW(pins, CB_RESETCONTENT, None, None);
        }
        for name in names {
            add_row(pins, name);
        }
        if names.is_empty() {
            add_row(pins, "Nothing pinned yet");
        }
        select(pins, Some(clamp_selection(selected as isize, names.len())));
        // Nothing to reorder until something is pinned.
        for id in [PIN, EARLIER, LATER] {
            // SAFETY: our own live child controls.
            let _ = unsafe { EnableWindow(self.h(id), !names.is_empty()) };
        }
    }
    /// The pin combo's current row (0 when none), so a refresh keeps the user's place.
    pub fn pin_selection(&self) -> usize {
        clamp_selection(
            chosen_row(self.h(PIN)).map_or(-1, |r| r as isize),
            usize::MAX,
        )
    }
    /// Selects pin row `row` (kept inside the list), so the next refresh keeps it.
    pub fn select_pin(&self, row: usize) {
        let pins = self.h(PIN);
        // SAFETY: synchronous query of our own combo box.
        let count = unsafe { SendMessageW(pins, CB_GETCOUNT, None, None) }.0;
        select(
            pins,
            Some(clamp_selection(row as isize, count.max(0) as usize)),
        );
    }
    pub fn take_move(&mut self) -> Option<(usize, bool)> {
        self.move_pin.take()
    }
    /// Fills the hidden-apps list; nothing to show until an app is hidden.
    pub fn set_hidden(&self, names: &[String]) {
        let list = self.h(HIDDEN);
        // SAFETY: synchronous string copies to our own native combo box.
        unsafe {
            SendMessageW(list, CB_RESETCONTENT, None, None);
        }
        for name in names {
            add_row(list, name);
        }
        if names.is_empty() {
            add_row(list, "Nothing hidden");
        }
        select(list, Some(0));
        for id in [HIDDEN, SHOW] {
            // SAFETY: our own live child controls.
            let _ = unsafe { EnableWindow(self.h(id), !names.is_empty()) };
        }
    }
    pub fn take_show(&mut self) -> Option<usize> {
        self.show_hidden.take()
    }

    /// One control row: the label before its control and the caption after it.
    fn add(
        &self,
        id: usize,
        kind: Kind,
        title: &str,
        caption: &str,
        page: usize,
        card: usize,
    ) -> Result<Item> {
        let window = self.contents[page];
        let (clip, flat) = (WS_CLIPSIBLINGS.0, WINDOW_EX_STYLE(0));
        let stat = |text: &str, id: usize| {
            control(window, w!("Static"), text, id, clip, WS_EX_NOPARENTNOTIFY)
        };
        let none = HWND::default();
        let (mut label, mut cap) = (none, none);
        let tall = !caption.is_empty() && kind != Kind::Btn;
        let h = match kind {
            Kind::Check => control(
                window,
                w!("Button"),
                title,
                id,
                BS_AUTOCHECKBOX as u32 | clip | if tall { BS_TOP as u32 } else { 0 },
                flat,
            )?,
            Kind::Combo(_) => {
                label = stat(title, LABEL)?;
                let combo = control(
                    window,
                    w!("ComboBox"),
                    "",
                    id,
                    CBS_DROPDOWNLIST as u32
                        | CBS_OWNERDRAWFIXED as u32
                        | CBS_HASSTRINGS as u32
                        | WS_VSCROLL.0
                        | clip,
                    flat,
                )?;
                // SAFETY: owned native control, subclass lasts exactly as long as the HWND.
                unsafe {
                    let _ = SetWindowSubclass(combo, Some(combo_proc), 1, 0);
                }
                combo
            }
            Kind::Edit(_) => {
                label = stat(title, LABEL)?;
                let extra = match id {
                    SHELF | WCFG..=186 | PCFG..=191 => ES_NUMBER as u32,
                    LIMSNIP => ES_READONLY as u32,
                    _ => 0,
                };
                let initial = match id {
                    TIMER => FIRST_TIMER,
                    LIMSNIP => crate::limits::SNIPPET,
                    _ => "",
                };
                control(
                    window,
                    w!("Edit"),
                    initial,
                    id,
                    ES_AUTOHSCROLL as u32 | clip | extra,
                    flat,
                )?
            }
            Kind::Btn => {
                if !caption.is_empty() {
                    label = stat(caption, LABEL)?;
                }
                control(window, w!("Button"), title, id, clip, flat)?
            }
            Kind::Note => {
                label = stat(title, LABEL)?;
                none
            }
        };
        if tall {
            cap = stat(caption, CAPTION)?;
        }
        let limit = match id {
            SHELF => 3,
            CITY => MAX_CITY,
            PSHIFT..=174 | WCFG..=186 | PCFG..=191 => 4,
            DIAGNOSTICS => MAX_PATH,
            TIMER => 8,
            _ => 0,
        };
        // SAFETY: scalar limits and strings on our own controls; Windows bounds what a user can type or paste.
        unsafe {
            if limit > 0 {
                SendMessageW(h, EM_SETLIMITTEXT, Some(WPARAM(limit)), Some(LPARAM(0)));
            }
        }
        fill_combo(h, id);
        Ok(Item {
            id,
            kind,
            page: page as u8,
            card: card as u8,
            h,
            label,
            cap,
            row: Cell::new((0, 0)),
        })
    }

    /// The display row's text: "Display 1, primary, 1920 by 1080 at 125%". The switch shows whether ROQA is on there.
    fn tile_name(&self, index: usize) -> String {
        let m = &self.monitors[index];
        format!(
            "Display {}{}, {} by {} at {}%",
            index + 1,
            if self.is_primary_display(index) {
                ", primary"
            } else {
                ""
            },
            m.rect.width,
            m.rect.height,
            m.dpi * 100 / 96,
        )
    }

    fn monitor_item(&self, index: usize) -> Result<Item> {
        let on = !self.current.off_monitors.contains(&self.devices[index]);
        let item = self.add(
            MONITOR + index,
            Kind::Check,
            &self.tile_name(index),
            "",
            0,
            DISPLAYS_CARD,
        )?;
        set_checked(item.h, on);
        Ok(item)
    }

    /// Replaces the monitor switches after the monitor list changed (a passive refresh: it fires no setting).
    pub fn refresh_monitors(&mut self, monitors: &[Monitor]) -> Result<()> {
        if self.monitors.iter().eq(monitors.iter().take(MAX_OFF)) {
            return Ok(());
        }
        let is_monitor = |i: &Item| (MONITOR..MONITOR + MAX_OFF).contains(&i.id);
        // SAFETY: scalar query of focus on the Settings thread.
        let focus = unsafe { GetFocus() };
        let focused = self
            .items
            .iter()
            .find(|item| is_monitor(item) && item.h == focus)
            .map(|item| {
                let index = item.id - MONITOR;
                (index, self.devices[index].clone())
            });
        self.set_monitors(monitors);
        for item in self.items.iter().filter(|i| is_monitor(i)) {
            // SAFETY: owned monitor switch, no retained references.
            unsafe { DestroyWindow(item.h)? };
        }
        self.items.retain(|i| !is_monitor(i));
        let at = self
            .items
            .iter()
            .position(|i| i.id == NOTCH)
            .map_or(0, |i| i + 1);
        for index in 0..self.devices.len() {
            let item = self.monitor_item(index)?;
            self.items.insert(at + index, item);
        }
        let font = self.font.as_ref().map(|f| f.handle);
        for item in self.items.iter().filter(|i| is_monitor(i)) {
            // SAFETY: the new child borrows the live font.
            unsafe {
                SendMessageW(
                    item.h,
                    WM_SETFONT,
                    font.map(|f| WPARAM(f.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
        }
        self.tab_stops();
        self.refresh_search(true);
        self.layout()?;
        self.reveal_search()?;
        if let Some((index, device)) = focused {
            let next = self
                .devices
                .iter()
                .position(|present| *present == device)
                .unwrap_or(index.min(self.devices.len().saturating_sub(1)));
            let focus = if self.devices.is_empty() {
                self.h(NOTCH)
            } else {
                self.h(MONITOR + next)
            };
            // SAFETY: focus returns to one of our own live controls without sending an action.
            unsafe {
                let _ = SetFocus(Some(focus));
            }
            self.reveal_focus()?;
        }
        Ok(())
    }

    /// Creates the controls: the rail and the compact picker, the local tabs, the viewport with one content
    /// window per page and each page's rows top to bottom, then the footer. Creation order is Tab order.
    fn build(&mut self) -> Result<()> {
        let window = self.window;
        let flat = WINDOW_EX_STYLE(0);
        self.side = control(
            window,
            w!("ListBox"),
            "",
            SIDE,
            (LBS_NOTIFY | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT) as u32
                | WS_GROUP.0,
            flat,
        )?;
        self.nav = control(
            window,
            w!("ComboBox"),
            "",
            NAV,
            CBS_DROPDOWNLIST as u32
                | CBS_OWNERDRAWFIXED as u32
                | CBS_HASSTRINGS as u32
                | WS_VSCROLL.0
                | WS_GROUP.0,
            flat,
        )?;
        // SAFETY: owned native control, subclass lasts exactly as long as the HWND.
        unsafe {
            let _ = SetWindowSubclass(self.nav, Some(combo_proc), 1, 0);
        }
        for route in &ROUTES {
            let name = wide(route.name);
            // SAFETY: the list box and the combo copy the terminated string synchronously.
            unsafe {
                let text = Some(LPARAM(name.as_ptr() as isize));
                SendMessageW(self.side, LB_ADDSTRING, None, text);
                SendMessageW(self.nav, CB_ADDSTRING, None, text);
            }
        }
        self.build_search()?;
        // One radio per page of a destination that has several; the first of each destination starts its group.
        for (index, page) in PAGES.iter().enumerate() {
            let tab = if page.name.is_empty() {
                HWND::default()
            } else {
                let first = index == pages_of(usize::from(page.route)).start;
                control(
                    window,
                    w!("Button"),
                    page.name,
                    TABBTN + index,
                    BS_AUTORADIOBUTTON as u32
                        | BS_PUSHLIKE as u32
                        | if first { WS_GROUP.0 } else { 0 },
                    flat,
                )?
            };
            self.tabs.push(tab);
        }
        self.viewport = view::viewport(window)?;
        self.search.body = view::page(self.viewport)?;
        for _ in PAGES {
            self.contents.push(view::page(self.viewport)?);
        }
        for (page, p) in PAGES.iter().enumerate() {
            for (card, c) in p.cards.iter().enumerate() {
                for row in c.rows {
                    let item = self.add(row.id, row.kind, row.title, row.caption, page, card)?;
                    self.items.push(item);
                    if row.id == NOTCH {
                        for index in 0..self.devices.len() {
                            let item = self.monitor_item(index)?;
                            self.items.push(item);
                        }
                    }
                }
            }
        }
        self.status = control(
            window,
            w!("Static"),
            DEFAULT_STATUS,
            STATUS,
            SS_NOTIFY | WS_CLIPSIBLINGS.0,
            WS_EX_NOPARENTNOTIFY,
        )?;
        self.close = control(
            window,
            w!("Button"),
            "Close",
            CLOSE,
            WS_CLIPSIBLINGS.0 | BS_OWNERDRAW as u32,
            flat,
        )?;
        self.tab_stops();
        self.refresh_agents();
        self.select_route();
        Ok(())
    }

    /// Brings the open window forward as it is: its page and scroll stay.
    pub fn raise(&self) {
        // SAFETY: this live dialog's own HWND. Windows keeps its normal foreground restrictions.
        unsafe {
            if IsIconic(self.window).as_bool() {
                let _ = ShowWindow(self.window, SW_RESTORE);
            }
            let _ = SetForegroundWindow(self.window);
        }
    }

    pub fn focus_city(&mut self) -> Result<()> {
        let page = self
            .items
            .iter()
            .find(|item| item.id == CITY)
            .map_or(0, |item| usize::from(item.page));
        self.show_page(page)?;
        // SAFETY: the control belongs to this live dialog. Respect foreground restrictions.
        unsafe {
            if GetForegroundWindow() == self.window {
                let _ = SetFocus(Some(self.h(CITY)));
            }
        }
        self.reveal_focus()
    }

    /// Shows destination `route` on the page it was last left on.
    fn show_route(&mut self, route: usize) -> Result<()> {
        self.show_page(self.last_page[route.min(ROUTES.len() - 1)])
    }

    /// Shows the page (`PAGES` index) and its destination: its content window alone is visible, at the offset it
    /// was left at, and the rail, picker and local tabs show where it is. The footer's message belongs to the
    /// page where its action happened, so it clears.
    fn show_page(&mut self, page: usize) -> Result<()> {
        self.leave_search(true)?;
        let page = page.min(PAGES.len() - 1);
        scroll::release();
        self.offsets[self.page] = self.scroll;
        // A focused control on the page being left would hide with it: focus goes to the rail's stop instead.
        // SAFETY: scalar queries and a focus move on our own live windows.
        unsafe {
            if self.page != page && IsChild(self.contents[self.page], GetFocus()).as_bool() {
                let _ = SetFocus(Some(if self.frame.compact {
                    self.nav
                } else {
                    self.side
                }));
            }
        }
        self.page = page;
        self.route = usize::from(PAGES[page].route);
        self.last_page[self.route] = page;
        self.scroll = self.offsets[page];
        self.wheel_remainder = 0;
        self.say("")?;
        self.select_route();
        self.layout()
    }

    /// Makes the controls say which page is shown: the rail row, the picker's row, the local tabs of this
    /// destination (the others hide) with the current one checked, and the one visible content window.
    fn select_route(&self) {
        let tabbed = pages_of(self.route);
        let searching = self.search.is_open();
        // SAFETY: scalar messages and visibility changes on our own live controls.
        unsafe {
            SendMessageW(self.side, LB_SETCURSEL, Some(WPARAM(self.route)), None);
            SendMessageW(self.nav, CB_SETCURSEL, Some(WPARAM(self.route)), None);
            for (index, &tab) in self.tabs.iter().enumerate() {
                if tab.0.is_null() {
                    continue;
                }
                let here = !searching && tabbed.contains(&index);
                let current = index == self.page;
                SendMessageW(tab, BM_SETCHECK, Some(WPARAM(usize::from(current))), None);
                // The group has one Tab stop: the current tab.
                let style = GetWindowLongPtrW(tab, GWL_STYLE);
                let stop = WS_TABSTOP.0 as isize;
                SetWindowLongPtrW(
                    tab,
                    GWL_STYLE,
                    if current { style | stop } else { style & !stop },
                );
                let _ = ShowWindow(tab, if here { SW_SHOWNA } else { SW_HIDE });
            }
            for (index, &content) in self.contents.iter().enumerate() {
                let command = if !searching && index == self.page {
                    SW_SHOWNA
                } else {
                    SW_HIDE
                };
                let _ = ShowWindow(content, command);
            }
            let _ = ShowWindow(
                self.search.body,
                if searching { SW_SHOWNA } else { SW_HIDE },
            );
        }
    }

    /// Writes `settings` into the controls (not the timer length, which is not a setting).
    fn show(&self, settings: &Settings) {
        let mut copy = settings.clone();
        for item in &self.items {
            let h = item.h;
            match item.kind {
                Kind::Check => {
                    if let Some(on) = flag(&mut copy, item.id) {
                        set_checked(h, *on);
                    } else if let Some(device) = self.devices.get(item.id.wrapping_sub(MONITOR)) {
                        let on = !settings.off_monitors.contains(device);
                        set_checked(h, on);
                        let _ = set_text(h, &self.tile_name(item.id - MONITOR));
                    }
                }
                Kind::Combo(_) => {
                    let row = match item.id {
                        THEME => row_of(&Theme::ALL, &settings.theme),
                        DOCKSTYLE => row_of(&DockStyle::ALL, &settings.dock_style),
                        UNITS => row_of(&Units::ALL, &settings.units),
                        PMETHOD => row_of(&Method::ALL, &settings.prayer_method),
                        PLEAD => row_of(&LEADS, &settings.prayer_lead),
                        LIMGROUP => usize::from(settings.limits_group),
                        WIDGET_LIMIT => settings.widget_limit - 1,
                        NETU => usize::from(settings.widgets.net_bits),
                        WEEK1 => usize::from(settings.widgets.week_sunday),
                        LOWAT => row_of(&LOW_ROWS, &settings.widgets.low_at),
                        DISKD => disk_row(settings.widgets.disk_drive),
                        NSIZE => row_of(&NOTCH_SIZES, &settings.notch_size),
                        NOPEN => usize::from(settings.open_click),
                        ANIM => usize::from(settings.anim_fast),
                        REVEAL => usize::from(settings.reveal_delay),
                        HIDE => usize::from(settings.hide_delay),
                        PSOUND => self.sound_row(&settings.prayer_sound),
                        _ => continue,
                    };
                    select(h, Some(row));
                }
                Kind::Edit(_) => {
                    let _ = match item.id {
                        SHELF => set_text(h, &settings.shelf_cap.to_string()),
                        PSHIFT..=174 => {
                            set_text(h, &settings.prayer_shift[item.id - PSHIFT].to_string())
                        }
                        WCFG..=186 => set_text(h, &settings.water_cfg[item.id - WCFG].to_string()),
                        PCFG..=191 => set_text(h, &settings.pomo_cfg[item.id - PCFG].to_string()),
                        CITY => set_text(h, &settings.city),
                        DIAGNOSTICS => set_text(h, settings.diagnostics.as_deref().unwrap_or("")),
                        _ => Ok(()),
                    };
                }
                _ => {}
            }
        }
    }

    /// The sound list row for a stored setting: silent is 0; the default is its file's row (or silent without one).
    fn sound_row(&self, setting: &str) -> usize {
        match prayer_tab::resolve(setting, &self.sounds) {
            None => 0,
            Some(path) => self
                .sounds
                .iter()
                .position(|s| s == path)
                .map_or(0, |i| i + 1),
        }
    }

    /// Fills the sound list: silent, the files found and, when the chosen file is elsewhere, that one too.
    fn fill_sounds(&mut self, setting: &str) {
        if !matches!(setting, "" | SILENT) && !self.sounds.iter().any(|s| s == setting) {
            self.sounds.push(setting.to_owned());
        }
        let combo = self.h(PSOUND);
        // SAFETY: our own combo box.
        unsafe {
            SendMessageW(combo, CB_RESETCONTENT, None, None);
        }
        add_row(combo, "None (silent)");
        for sound in &self.sounds {
            let missing = prayer_tab::missing(sound);
            let label = prayer_tab::label(sound);
            add_row(
                combo,
                &if missing {
                    format!("{label} (file missing)")
                } else {
                    label.to_owned()
                },
            );
        }
    }

    /// The host stored a file the user browsed to: list it and select it.
    pub fn set_sound(&mut self, setting: &str) {
        self.current.prayer_sound = setting.to_owned();
        self.fill_sounds(setting);
        select(self.h(PSOUND), Some(self.sound_row(setting)));
    }

    /// Quick settings flipped the theme while this window is open: the box and the colours follow, and the
    /// window's own copy keeps it, so the next edit here does not flip it back.
    pub fn set_theme(&mut self, theme: Theme) -> Result<()> {
        self.current.theme = theme;
        select(self.h(THEME), Some(row_of(&Theme::ALL, &theme)));
        self.relayout()
    }

    /// What the user asked of the sound since the host last asked.
    pub fn take_sound_action(&mut self) -> Option<SoundAction> {
        self.sound_action.take()
    }

    /// Reads the controls, brings them into range, shows what was kept and hands a real change to the host.
    fn commit(&mut self) -> Result<()> {
        let mut next = self.current.clone();
        let mut off = Vec::new();
        let mut invalid = false;
        for item in &self.items {
            let h = item.h;
            match item.kind {
                Kind::Check => {
                    if let Some(on) = flag(&mut next, item.id) {
                        *on = checked(h);
                    } else if !checked(h) {
                        off.extend(self.devices.get(item.id.wrapping_sub(MONITOR)).cloned());
                    }
                }
                Kind::Combo(_) => {
                    let row = chosen_row(h);
                    match item.id {
                        WIDGET_LIMIT => {
                            if let Some(row) = row {
                                next.widget_limit = row + 1;
                            }
                        }
                        NETU => {
                            next.widgets.net_bits = row.map_or(next.widgets.net_bits, |r| r == 1)
                        }
                        WEEK1 => {
                            next.widgets.week_sunday =
                                row.map_or(next.widgets.week_sunday, |r| r == 1)
                        }
                        LOWAT => pick(&LOW_ROWS, &mut next.widgets.low_at, row),
                        DISKD => disk_pick(&mut next.widgets.disk_drive, row),
                        THEME => pick(&Theme::ALL, &mut next.theme, row),
                        DOCKSTYLE => pick(&DockStyle::ALL, &mut next.dock_style, row),
                        UNITS => pick(&Units::ALL, &mut next.units, row),
                        PMETHOD => pick(&Method::ALL, &mut next.prayer_method, row),
                        PLEAD => pick(&LEADS, &mut next.prayer_lead, row),
                        NSIZE => pick(&NOTCH_SIZES, &mut next.notch_size, row),
                        NOPEN => next.open_click = row.map_or(next.open_click, |r| r == 1),
                        ANIM => next.anim_fast = row.map_or(next.anim_fast, |r| r == 1),
                        REVEAL => next.reveal_delay = row.map_or(next.reveal_delay, |r| r as u8),
                        HIDE => next.hide_delay = row.map_or(next.hide_delay, |r| r as u8),
                        LIMGROUP => {
                            next.limits_group = row.map_or(next.limits_group, |r| r.min(1) as u8)
                        }
                        // A row differing from what the stored setting shows is a pick; otherwise "default" stays "default".
                        PSOUND => {
                            if let Some(row) =
                                row.filter(|&r| r != self.sound_row(&next.prayer_sound))
                            {
                                next.prayer_sound = match row {
                                    0 => SILENT.to_owned(),
                                    n => self.sounds.get(n - 1).cloned().unwrap_or_default(),
                                };
                            }
                        }
                        _ => {}
                    }
                }
                Kind::Edit(_) => match item.id {
                    SHELF => next.shelf_cap = text(h, 3).trim().parse().unwrap_or(next.shelf_cap),
                    CITY => next.city = text(h, MAX_CITY),
                    PCFG..=191 => {
                        let slot = &mut next.pomo_cfg[item.id - PCFG];
                        *slot = text(h, 4).trim().parse().unwrap_or(*slot);
                    }
                    WCFG..=186 => {
                        let slot = &mut next.water_cfg[item.id - WCFG];
                        *slot = text(h, 4).trim().parse().unwrap_or(*slot);
                    }
                    PSHIFT..=174 => {
                        let slot = &mut next.prayer_shift[item.id - PSHIFT];
                        let (minutes, valid) = parse_shift(&text(h, 4), *slot);
                        (*slot, invalid) = (minutes, invalid || !valid);
                    }
                    DIAGNOSTICS => next.diagnostics = Some(text(h, MAX_PATH)),
                    _ => {}
                },
                _ => {}
            }
        }
        // A monitor that is not connected now keeps its stored state.
        next.off_monitors.retain(|d| !self.devices.contains(d));
        next.off_monitors.extend(off);
        let kept_a_kind = next.dock || next.notch;
        let ranged = next.clamped();
        let mut next = ranged.clone().keep_one_monitor(&self.devices);
        if next != ranged
            && let Some(active) = self
                .devices
                .iter()
                .find(|device| !self.current.off_monitors.contains(device))
        {
            next = ranged
                .clone()
                .keep_one_monitor(std::slice::from_ref(active));
        }
        self.show(&next);
        let changed = next != self.current;
        if let Some(status) = footer_status(invalid, kept_a_kind, ranged != next, changed) {
            self.say(status)?;
        }
        if next != self.current {
            let restyle =
                next.theme != self.current.theme || next.match_accent != self.current.match_accent;
            self.current = next.clone();
            self.change = Some(next);
            if restyle {
                self.relayout()?;
            }
        }
        Ok(())
    }

    /// The footer's line; empty gives its resting words.
    pub fn say(&self, status: &str) -> Result<()> {
        set_text(
            self.status,
            if status.is_empty() {
                DEFAULT_STATUS
            } else {
                status
            },
        )
    }
    /// Start at sign-in could not be changed: the switch goes back.
    pub fn set_autostart(&mut self, on: bool) {
        self.current.autostart = on;
        self.show(&self.current);
    }

    /// The newest settings the user chose since the host last asked.
    pub fn take_change(&mut self) -> Option<Settings> {
        self.change.take()
    }

    pub fn take_pomo(&mut self) -> Option<bool> {
        self.pomo.take()
    }

    /// The user pressed "I drank a glass".
    pub fn take_drank(&mut self) -> bool {
        std::mem::take(&mut self.drank)
    }

    /// Today's water progress, shown beside the button.
    pub fn set_water(&self, progress: &str) {
        if let Some(item) = self.items.iter().find(|i| i.id == WDRANK)
            && text(item.label, 64) != progress
        {
            let _ = set_text(item.label, progress);
        }
    }

    /// A timer length in seconds the user asked to start.
    pub fn take_start(&mut self) -> Option<u64> {
        self.start.take()
    }

    fn any_combo_dropped(&self) -> bool {
        let dropped = |h: HWND| {
            // SAFETY: scalar query of our own live combo box.
            unsafe { SendMessageW(h, CB_GETDROPPEDSTATE, None, None).0 != 0 }
        };
        dropped(self.nav)
            || self
                .items
                .iter()
                .any(|i| matches!(i.kind, Kind::Combo(_)) && dropped(i.h))
    }

    pub fn message(&mut self, message: &MSG) -> Result<bool> {
        if message.message == WM_MOUSEWHEEL
            && (message.hwnd == self.window
                // SAFETY: scalar check of our own live dialog window.
                || unsafe { IsChild(self.window, message.hwnd) }.as_bool())
            && !self.any_combo_dropped()
        {
            // The wheel scrolls the page only while the pointer is over the viewport or its bar; over the rail,
            // the heading and the footer it does nothing (and no control under it takes it).
            let mut at = message.pt;
            // SAFETY: a scalar conversion for our own live window.
            let over = unsafe { ScreenToClient(self.window, &mut at) }.as_bool()
                && self.frame.scrolls_at(at.x, at.y);
            if over {
                let delta = i32::from((message.wParam.0 >> 16) as i16);
                self.scroll_message(WHEEL, delta)?;
            }
            return Ok(true);
        }
        if message.hwnd != self.window || (WM_KEYFIRST..=WM_KEYLAST).contains(&message.message) {
            return self.key(message);
        }
        match message.message {
            ACTION => {
                let id = message.wParam.0 & 0xffff;
                let notification = (message.wParam.0 >> 16) as u32;
                let kind = self.items.iter().find(|i| i.id == id).map(|i| i.kind);
                match (id, kind) {
                    (search::EDIT, _) if notification == EN_CHANGE => self.search_changed()?,
                    (search::EDIT, _) if matches!(notification, EN_SETFOCUS | EN_KILLFOCUS) => {
                        // SAFETY: our live dialog and the search field's client rectangle.
                        unsafe {
                            let _ =
                                InvalidateRect(Some(self.window), Some(&self.frame.search), true);
                        }
                    }
                    (SIDE, _) if notification == LBN_SELCHANGE => {
                        // SAFETY: scalar query of our own list box.
                        let row = unsafe { SendMessageW(self.side, LB_GETCURSEL, None, None) }.0;
                        self.show_route(row.max(0) as usize)?;
                    }
                    (NAV, _) if notification == CBN_SELCHANGE => {
                        self.show_route(chosen_row(self.nav).unwrap_or(0))?;
                    }
                    (TABBTN..=0x1ff, _) if notification == BN_CLICKED => {
                        self.show_page(id - TABBTN)?;
                    }
                    (PBROWSE | PTEST, _) if notification == BN_CLICKED => {
                        self.sound_action = Some(if id == PTEST {
                            SoundAction::Test
                        } else {
                            SoundAction::Browse
                        });
                    }
                    (EARLIER | LATER, _) if notification == BN_CLICKED => {
                        if let Some(index) = chosen_row(self.h(PIN)) {
                            self.move_pin = Some((index, id == LATER));
                        }
                    }
                    (SHOW, _) if notification == BN_CLICKED => {
                        self.show_hidden = chosen_row(self.h(HIDDEN));
                    }
                    (WDRANK, _) if notification == BN_CLICKED => self.drank = true,
                    (AGENT_BTN..=221, _) if notification == BN_CLICKED => {
                        self.agent_click(id - AGENT_BTN)
                    }
                    (ASK_BTN, _) if notification == BN_CLICKED => self.agent_click(7),
                    (ADD_ALL, _) if notification == BN_CLICKED => self.agent_add_all(),
                    (LIMCOPY, _) if notification == BN_CLICKED => {
                        // SAFETY: select-all then copy on our own read-only edit.
                        unsafe {
                            let edit = self.h(LIMSNIP);
                            SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
                            SendMessageW(edit, WM_COPY, None, None);
                        }
                        self.say("Copied.")?;
                    }
                    (PSTART | PSKIP, _) if notification == BN_CLICKED => {
                        self.pomo = Some(id == PSKIP)
                    }
                    (START, _) if notification == BN_CLICKED => {
                        match parse_timer(&text(self.h(TIMER), 8)) {
                            Some(seconds) => self.start = Some(seconds),
                            None => self.say(
                                "Enter minutes, m:ss or h:mm:ss, from 1 second to 24 hours.",
                            )?,
                        }
                    }
                    (RESET, _) if notification == BN_CLICKED => {
                        // SAFETY: the live settings dialog owns this confirmation.
                        let answer = unsafe {
                            MessageBoxW(
                                Some(self.window),
                                w!("Restore ROQA settings to their defaults?"),
                                w!("Reset to defaults"),
                                MB_YESNO | MB_DEFBUTTON2 | MB_ICONQUESTION,
                            )
                        };
                        if answer == IDYES {
                            self.show(&Settings::default());
                            self.commit()?;
                        }
                    }
                    (CLOSE, _) => {
                        self.commit()?; // an edit still being typed is kept
                        self.closed = true;
                        restore_foreground(self.window, self.return_to);
                    }
                    (OK, _) => self.commit()?,
                    (_, Some(Kind::Check)) if notification == BN_CLICKED => self.commit()?,
                    (_, Some(Kind::Combo(_))) if notification == CBN_SELCHANGE => self.commit()?,
                    (_, Some(Kind::Edit(_))) if notification == EN_KILLFOCUS => self.commit()?,
                    _ => {}
                }
            }
            RELAYOUT => self.relayout()?,
            search::OPEN => self.open_search_hit(Some((message.wParam.0, message.lParam.0)))?,
            SCROLL => self.scroll_message(message.wParam.0, message.lParam.0 as i32)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    /// The Windows text size changed.
    pub fn set_factor(&mut self, factor: f64) -> Result<()> {
        self.factor = factor;
        self.relayout()
    }

    /// New font (DPI or text size changed): applies it to every control, sizes the window, then lays out.
    fn relayout(&mut self) -> Result<()> {
        let work = self.work;
        scroll::release();
        // SAFETY: owned dialog is live.
        let dpi = unsafe { GetDpiForWindow(self.window) }.max(96);
        let font = PickerFont::create(dpi, self.factor)?;
        self.pal = palette(&self.current);
        let dark = BOOL::from(self.pal.dark);
        // SAFETY: a scalar attribute on our own window; older Windows ignores it.
        unsafe {
            let _ = DwmSetWindowAttribute(
                self.window,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&dark as *const BOOL).cast(),
                size_of::<BOOL>() as u32,
            );
            // Match the caption on every theme change, including a return from dark to Light.
            // Older Windows ignores both attributes.
            for (attribute, value) in [
                (DWMWA_CAPTION_COLOR, self.pal.side),
                (DWMWA_TEXT_COLOR, self.pal.text),
            ] {
                let colour = rgb(value);
                let _ = DwmSetWindowAttribute(
                    self.window,
                    attribute,
                    (&colour as *const COLORREF).cast(),
                    size_of::<COLORREF>() as u32,
                );
            }
        }
        // Each child with whether it is an edit or a drop-down (they take the dark "CFD" look).
        let field = |i: &Item| matches!(i.kind, Kind::Combo(_) | Kind::Edit(_));
        let handles = self
            .items
            .iter()
            .flat_map(|i| [(i.h, field(i)), (i.label, false), (i.cap, false)])
            .chain([(self.nav, true), (self.search.edit, true)])
            .chain(
                [self.side, self.status, self.close]
                    .into_iter()
                    .chain(self.tabs.iter().copied())
                    .map(|h| (h, false)),
            )
            .filter(|(h, _)| !h.0.is_null());
        for (child, field) in handles {
            // The native look of lists, edits, drop-downs and buttons follows the app mode too.
            // SAFETY: our own live child.
            unsafe {
                let _ = SetWindowTheme(
                    child,
                    match (self.pal.dark, field) {
                        (false, _) => PCWSTR::null(),
                        (true, true) => w!("DarkMode_CFD"),
                        (true, false) => w!("DarkMode_Explorer"),
                    },
                    PCWSTR::null(),
                );
            }
            // SAFETY: child controls are live and borrow this font until replaced or destroyed.
            unsafe {
                SendMessageW(
                    child,
                    WM_SETFONT,
                    Some(WPARAM(font.handle.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
        }
        let scale = font.scale;
        self.font = Some(font);
        let px = |v: i32| (f64::from(v) * scale).round() as i32;
        let mut outer = RECT {
            right: px(DESIGN_W),
            bottom: px(DESIGN_H),
            ..Default::default()
        };
        // SAFETY: fills the local RECT for the dialog's fixed style at this DPI; positions only our own window.
        unsafe {
            AdjustWindowRectExForDpi(
                &mut outer,
                WINDOW_STYLE(DIALOG_STYLE),
                false,
                WINDOW_EX_STYLE(0),
                dpi,
            )?;
            // The preferred size, kept 12 DIPs inside the work area when it can be, never larger than the area.
            let room = |size: i32, area: i32| size.min(area - 2 * px(12)).min(area).max(1);
            let width = room(outer.right - outer.left, work.width);
            let height = room(outer.bottom - outer.top, work.height);
            let style = GetWindowLongPtrW(self.window, GWL_STYLE);
            SetWindowLongPtrW(self.window, GWL_STYLE, style | WS_CLIPCHILDREN.0 as isize);
            SetWindowPos(
                self.window,
                None,
                work.x + (work.width - width) / 2,
                work.y + (work.height - height) / 2,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )?;
        }
        // Its repaint comes after PAINT has the new tokens, not during WM_SETFONT with the old ones.
        self.layout()
    }

    /// Lays out with drawing frozen, then repaints once (frame and scroll bar too): moving and showing each
    /// control on screen showed the page coming apart row by row. Runs for a new page, size, font or text, never
    /// for a scroll.
    fn layout(&mut self) -> Result<()> {
        let window = self.window;
        // SAFETY: our own live window; WM_SETREDRAW TRUE also shows a window, so a hidden one is left alone.
        let shown = unsafe { IsWindowVisible(window) }.as_bool();
        if !shown {
            return self.arrange();
        }
        // SAFETY: a scalar message to our own live window.
        unsafe { SendMessageW(window, WM_SETREDRAW, Some(WPARAM(0)), None) };
        let placed = self.arrange();
        let all = RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_FRAME;
        // SAFETY: our own live window and children.
        unsafe {
            SendMessageW(window, WM_SETREDRAW, Some(WPARAM(1)), None);
            let _ = RedrawWindow(Some(window), None, None, all);
        }
        placed
    }

    /// Places the rail or the picker, the local tabs, the viewport with the current page's content window, and
    /// the footer, and hands the painters what to draw.
    fn arrange(&mut self) -> Result<()> {
        let Some(font) = self.font.as_ref() else {
            return Ok(());
        };
        let (handle, scale) = (font.handle, font.scale);
        let px = |v: i32| (f64::from(v) * scale).round() as i32;
        let mut client = RECT::default();
        // SAFETY: our own live window.
        unsafe { GetClientRect(self.window, &mut client)? };
        let tabs = pages_of(self.route);
        let frame = view::frame(
            client.right,
            client.bottom,
            scale,
            tabs.len() > 1 && !self.search.is_open(),
        );
        let (width, view) = (
            frame.viewport.right - frame.viewport.left,
            frame.viewport.bottom - frame.viewport.top,
        );
        let results = self.search.results(scale, frame.compact);
        let (geometry, content) = if let Some(results) = &results {
            (Geometry::default(), results.height())
        } else {
            self.place(self.page, scale, width, handle)?
        };
        self.frame = frame;
        self.scroll_max = (content - view).max(0);
        self.scroll = self.scroll.clamp(0, self.scroll_max);
        // SAFETY: our own live window and children.
        unsafe {
            let put = |h: HWND, r: RECT| {
                SetWindowPos(
                    h,
                    None,
                    r.left,
                    r.top,
                    (r.right - r.left).max(1),
                    (r.bottom - r.top).max(1),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
            let show = |h: HWND, on: bool| {
                let _ = ShowWindow(h, if on { SW_SHOWNA } else { SW_HIDE });
            };
            show(self.side, !frame.compact);
            show(self.nav, frame.compact);
            if frame.compact {
                place::size_combo(self.nav, frame.picker.bottom - frame.picker.top, px(36));
                put(self.nav, frame.picker)?;
            } else {
                put(self.side, frame.rail)?;
            }
            SendMessageW(
                self.side,
                LB_SETITEMHEIGHT,
                Some(WPARAM(0)),
                Some(LPARAM(px(44) as isize)),
            );
            // The local tabs, side by side: the text and 28 DIPs of padding, 12 more for the selected dot.
            let dc = GetDC(Some(self.window));
            SelectObject(dc, handle.into());
            let mut x = frame.tabs.left;
            for index in tabs.filter(|_| frame.tabs.bottom > frame.tabs.top) {
                let mut words: Vec<u16> = PAGES[index].name.encode_utf16().collect();
                let mut measured = RECT::default();
                DrawTextW(dc, &mut words, &mut measured, DT_CALCRECT | DT_SINGLELINE);
                let wide = measured.right + px(40);
                put(
                    self.tabs[index],
                    RECT {
                        left: x,
                        right: x + wide,
                        ..frame.tabs
                    },
                )?;
                x += wide + px(6);
            }
            ReleaseDC(Some(self.window), dc);
            put(self.viewport, frame.viewport)?;
            put(
                self.search.edit,
                RECT {
                    left: frame.search.left + px(31),
                    top: frame.search.top + px(6),
                    right: frame.search.right - px(10),
                    bottom: frame.search.bottom - px(5),
                },
            )?;
            SetWindowPos(
                self.scrolling_body(),
                None,
                0,
                -self.scroll,
                width.max(1),
                content.max(view).max(1),
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS,
            )?;
            put(self.status, frame.status)?;
            put(self.close, frame.close)?;
            // The title font is made once per body font (a scroll never needs it).
            let old = PAINT.with(|p| p.take());
            let (title_font, bold, display_meta, display_number) = match &old {
                Some(old) if old.font == handle => (
                    old.title_font,
                    old.bold,
                    old.display_meta,
                    old.display_number,
                ),
                _ => {
                    if let Some(old) = &old {
                        let _ = DeleteObject(old.title_font.into());
                        let _ = DeleteObject(old.bold.into());
                        let _ = DeleteObject(old.display_meta.into());
                        let _ = DeleteObject(old.display_number.into());
                    }
                    let mut face = LOGFONTW::default();
                    GetObjectW(
                        handle.into(),
                        size_of::<LOGFONTW>() as i32,
                        Some((&mut face as *mut LOGFONTW).cast()),
                    );
                    face.lfHeight = -px(12);
                    let display_meta = CreateFontIndirectW(&face);
                    face.lfWeight = 600;
                    face.lfHeight = -px(26);
                    let display_number = CreateFontIndirectW(&face);
                    face.lfHeight = -px(15);
                    let bold = CreateFontIndirectW(&face);
                    face.lfHeight *= 2;
                    (
                        CreateFontIndirectW(&face),
                        bold,
                        display_meta,
                        display_number,
                    )
                }
            };
            let route = &ROUTES[self.route];
            PAINT.with(|p| {
                p.replace(Some(Paint {
                    pal: self.pal,
                    scale,
                    font: handle,
                    title_font,
                    bold,
                    display_meta,
                    display_number,
                    title: route.name,
                    caption: route.caption,
                    frame,
                    geometry,
                    results,
                    bar: scroll::Bar::new(frame.gutter, content, view, self.scroll, scale),
                }))
            });
        }
        Ok(())
    }
}

impl Drop for SettingsWindow {
    fn drop(&mut self) {
        if let Some(paint) = PAINT.with(|p| p.take()) {
            for font in [
                paint.title_font,
                paint.bold,
                paint.display_meta,
                paint.display_number,
            ] {
                // SAFETY: this owned font is no longer available to the painter.
                let _ = unsafe { DeleteObject(font.into()) };
            }
        }
        // SAFETY: sole owner; destroying the parent synchronously destroys all child controls before the font drops.
        unsafe {
            if let Err(error) = DestroyWindow(self.window) {
                eprintln!("Settings window cleanup: {error}");
            }
        }
    }
}

/// A typed prayer shift: the minutes and whether the text was valid. A number outside -30..=30 is brought to
/// the nearest end; text that is not a whole number keeps `old`. Either way `false` makes the window say so.
fn parse_shift(text: &str, old: i8) -> (i8, bool) {
    let max = i32::from(MAX_SHIFT);
    match text.trim().parse::<i32>() {
        Ok(n) => (n.clamp(-max, max) as i8, (-max..=max).contains(&n)),
        Err(_) => (old, false),
    }
}

/// The footer line after an edit, if any. `kept_a_kind` is whether the values the user asked for keep the dock or
/// the notch on (not the clamped ones: a refused switch snaps back, so the clamped result looks unchanged),
/// `monitor_kept` whether the last monitor was kept on, `changed` whether anything differs from before.
fn footer_status(
    invalid_shift: bool,
    kept_a_kind: bool,
    monitor_kept: bool,
    changed: bool,
) -> Option<&'static str> {
    if invalid_shift {
        // The box already shows the value that was kept, so the message explains the change.
        Some("A shift is whole minutes from -30 to 30; the nearest is kept.")
    } else if !kept_a_kind {
        Some("Keep the dock or the notch on.")
    } else if monitor_kept {
        Some("Keep at least one monitor on.")
    } else if changed {
        Some("Applied.")
    } else {
        None
    }
}

mod displays;
mod draw;
mod keyboard;
mod paint;
mod place;
mod routes;
mod scroll;
mod search;
mod view;
#[cfg(test)]
mod window_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_survives_empty_text() {
        use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC};
        let r = RECT {
            right: 10,
            bottom: 10,
            ..Default::default()
        };
        // SAFETY: a memory DC made and deleted here.
        unsafe {
            let dc = CreateCompatibleDC(None);
            write(dc, "", r, 0, DT_LEFT | DT_END_ELLIPSIS);
            let _ = DeleteDC(dc);
        }
    }
    use crate::{dock::Button, tabs::Tab};

    #[test]
    fn a_typed_shift_is_checked_and_a_bad_one_is_reported() {
        assert_eq!(parse_shift(" -30 ", 0), (-30, true));
        assert_eq!(parse_shift("+30", 0), (30, true));
        assert_eq!(parse_shift("0", 5), (0, true));
        // Out of range: the nearest end, reported. Not a number (or empty, or huge): old value, reported.
        assert_eq!(parse_shift("45", 0), (30, false));
        assert_eq!(parse_shift("-999999", 0), (-30, false));
        assert_eq!(parse_shift("abc", 7), (7, false));
        assert_eq!(parse_shift("", 7), (7, false));
        assert_eq!(parse_shift("1.5", -2), (-2, false));
        assert_eq!(parse_shift("99999999999", 3), (3, false));
    }

    #[test]
    fn pin_row_stays_inside_a_refreshed_list() {
        assert_eq!(clamp_selection(2, 5), 2);
        // A pin was removed while the last row was selected: the row follows the shorter list.
        assert_eq!(clamp_selection(4, 3), 2);
        // CB_ERR (-1) and an empty list both give row 0.
        assert_eq!((clamp_selection(-1, 3), clamp_selection(3, 0)), (0, 0));
    }

    fn rows() -> Vec<&'static Row> {
        PAGES
            .iter()
            .flat_map(|p| p.cards)
            .flat_map(|c| c.rows)
            .collect()
    }

    #[test]
    fn every_control_id_appears_on_exactly_one_page_and_one_row() {
        let mut ids: Vec<usize> = rows()
            .iter()
            .filter(|r| r.kind != Kind::Note)
            .map(|r| r.id)
            .collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "an id is on two rows");
    }

    #[test]
    fn every_tab_button_and_toggle_is_a_row_with_a_setting() {
        let ids: Vec<usize> = rows().iter().map(|r| r.id).collect();
        let mut s = Settings::default();
        for tab in Tab::ALL.into_iter().filter(|tab| *tab != Tab::Widgets) {
            assert!(
                ids.contains(&(TAB + tab.index())) && flag(&mut s, TAB + tab.index()).is_some()
            );
        }
        for button in Button::ALL {
            assert!(
                ids.contains(&(DBTN + button.index()))
                    && flag(&mut s, DBTN + button.index()).is_some()
            );
        }
        for row in rows().iter().filter(|r| r.kind == Kind::Check) {
            assert!(flag(&mut s, row.id).is_some(), "{}", row.title);
        }
        assert!(flag(&mut s, TAB + 9).is_none() && flag(&mut s, MONITOR).is_none());
    }

    #[test]
    fn the_fixed_lists_line_up_with_their_settings() {
        assert_eq!(LEAD_ROWS.len(), LEADS.len());
        assert_eq!(NOTCH_SIZES.len(), 4);
    }

    #[test]
    fn a_refused_last_switch_says_so_instead_of_applied() {
        // Asked for neither dock nor notch: refused even though the clamped result equals the current one.
        let refused = Some("Keep the dock or the notch on.");
        assert_eq!(footer_status(false, false, false, false), refused);
        assert_eq!(footer_status(false, false, false, true), refused);
        assert_eq!(footer_status(false, true, false, true), Some("Applied."));
        assert_eq!(footer_status(false, true, false, false), None);
        assert_eq!(
            footer_status(false, true, true, true),
            Some("Keep at least one monitor on.")
        );
        assert_eq!(
            footer_status(true, false, false, true),
            Some("A shift is whole minutes from -30 to 30; the nearest is kept.")
        );
    }
}
