//! The settings window: a sidebar of pages and a card pane, built from the `PAGES` table (one loop creates the
//! controls, one lays them out, `commit`/`show` read and write them by id). Standard child controls give tab
//! order, mnemonics, Narrator and text scaling; only the background, cards and sidebar are drawn here, in GDI
//! with stock objects. It owns no timer or thread: it exists only while open and does work only in the message
//! loop turns its own controls cause. A change goes to the host (`take_change`) which applies it live; a timer
//! request likewise (`take_start`).

use crate::{
    dock::{foreground_key, restore_foreground},
    layout::DockStyle,
    native_window,
    picker::{
        ACTION, CLOSE, DIALOG_STYLE, RELAYOUT, SCROLL, control, dialog_proc, empty_dialog, wide,
    },
    picker_font::PickerFont,
    prayer::Method,
    prayer_os,
    prayer_tab::{self, LEADS, SILENT},
    settings::{MAX_OFF, MAX_PATH, MAX_SHIFT, NOTCH_SIZES, Settings, parse_timer},
    theme::Theme,
    topology::{self, Monitor},
    weather::{MAX_CITY, Units},
    window_model::WindowKey,
};
use std::cell::RefCell;
use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmGetColorizationColor, DwmSetWindowAttribute},
            Gdi::*,
        },
        System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            Controls::*,
            HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
            Input::KeyboardAndMouse::{GetFocus, SetFocus},
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
const CAPTION: usize = 119;
const MONITOR: usize = 120;
/// The "Displays" card (index in page 0): the monitor tiles are drawn on a map instead of in rows.
const MAP_CARD: usize = 1;
/// DIPs: the map's height.
const MAP_H: i32 = 120;
/// One checkbox per dock button (`dock::Button::ALL` order).
const DBTN: usize = 130;
/// One checkbox per notch tab (`Tab::ALL` order), then the mascot eyes and the privacy dots.
const TAB: usize = 150;
const EYES: usize = TAB + 6;
const PRIVACY: usize = TAB + 7;
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
/// The five prayer fine-tune edits (Fajr, Dhuhr, Asr, Maghrib, Isha), then the notch size, open and animation
/// lists, and the full-screen, reminder-sound and prayer-pause switches.
const PSHIFT: usize = 170;
const NSIZE: usize = 175;
const NOPEN: usize = 176;
const ANIM: usize = 177;
const FSHIDE: usize = 178;
const RSOUNDS: usize = 179;
const PPAUSE: usize = 180;
const SIDE: usize = 98;
const OK: usize = 1;
/// `SS_CENTERIMAGE | SS_NOTIFY`: the footer strip centres its text and takes clicks, so scrolled rows cannot be hit under it.
const SS_CENTERIMAGE_NOTIFY: u32 = 0x300;
/// Timer length shown when the window opens.
const FIRST_TIMER: &str = "5";
/// DIPs: sidebar width, content gutter, footer strip, design client size.
const SIDE_W: i32 = 192;
const GUTTER: i32 = 22;
const FOOT: i32 = 44;

/// What the Settings window asks the host to do with the prayer sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundAction {
    Browse,
    Test,
}

/// How a row's control looks: the title is its accessible name (the checkbox text, or the label before it).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Check,
    /// A drop-down list of this many DIPs wide.
    Combo(i32),
    Edit(i32),
    /// A push button; the row caption is the text on its left.
    Btn,
    /// Text only.
    Note,
}
struct Row {
    id: usize,
    kind: Kind,
    title: &'static str,
    caption: &'static str,
}
struct Card {
    label: &'static str,
    rows: &'static [Row],
}
struct Page {
    name: &'static str,
    cards: &'static [Card],
}
const fn r(id: usize, kind: Kind, title: &'static str, caption: &'static str) -> Row {
    Row {
        id,
        kind,
        title,
        caption,
    }
}
const C: Kind = Kind::Check;

/// Every setting appears once, in tab order. Monitors are added at runtime after `NOTCH`.
const PAGES: &[Page] = &[
    Page {
        name: "General",
        cards: &[
            Card {
                label: "Show ROQA",
                rows: &[
                    r(DOCK, C, "Show the &dock", ""),
                    r(NOTCH, C, "Show the &notch", ""),
                ],
            },
            Card {
                label: "Displays",
                rows: &[],
            },
            Card {
                label: "Behaviour",
                rows: &[
                    r(DESKTOP, C, "Show only windows on this des&ktop", ""),
                    r(
                        FSHIDE,
                        C,
                        "Hide in &full screen",
                        "Hide the dock and notch over a full-screen app",
                    ),
                    r(
                        RSOUNDS,
                        C,
                        "Reminder s&ounds",
                        "Master switch for the timer and prayer sounds",
                    ),
                ],
            },
        ],
    },
    Page {
        name: "Appearance",
        cards: &[
            Card {
                label: "Colours",
                rows: &[r(THEME, Kind::Combo(170), "T&heme", "")],
            },
            Card {
                label: "Notch",
                rows: &[
                    r(
                        NSIZE,
                        Kind::Combo(120),
                        "Notch si&ze",
                        "Auto is the designed size",
                    ),
                    r(
                        NOPEN,
                        Kind::Combo(120),
                        "&Open the notch",
                        "By hovering or by a click",
                    ),
                    r(ANIM, Kind::Combo(120), "A&nimation speed", ""),
                ],
            },
            Card {
                label: "Dock",
                rows: &[r(DOCKSTYLE, Kind::Combo(170), "Dock st&yle", "")],
            },
        ],
    },
    Page {
        name: "Notch",
        cards: &[
            Card {
                label: "Tabs",
                rows: &[
                    r(TAB, C, "&Media tab", ""),
                    r(TAB + 1, C, "&Weather tab", ""),
                    r(TAB + 2, C, "Prayer tab", ""),
                    r(TAB + 3, C, "Timer ta&b", ""),
                    r(TAB + 4, C, "S&helf tab", ""),
                    r(TAB + 5, C, "Limits tab", ""),
                ],
            },
            Card {
                label: "In the notch",
                rows: &[
                    r(EYES, C, "Mascot e&yes", ""),
                    r(PRIVACY, C, "Privacy &indicator", ""),
                ],
            },
            Card {
                label: "Timer",
                rows: &[
                    r(SOUND, C, "Play a s&ound when a timer finishes", ""),
                    r(
                        TIMER,
                        Kind::Edit(120),
                        "&Timer length",
                        "Minutes, m:ss or h:mm:ss",
                    ),
                    r(START, Kind::Btn, "St&art timer", ""),
                ],
            },
            Card {
                label: "Shelf",
                rows: &[r(SHELF, Kind::Edit(70), "Shel&f size", "Entries kept")],
            },
        ],
    },
    Page {
        name: "Dock & edges",
        cards: &[
            Card {
                label: "Dock buttons",
                rows: &[
                    r(DBTN, C, "Start butto&n", ""),
                    r(DBTN + 1, C, "Searc&h button", ""),
                    r(DBTN + 2, C, "Task Vie&w button", ""),
                    r(DBTN + 3, C, "Minimized apps &list", ""),
                    r(DBTN + 4, C, "Show deskto&p strip", ""),
                ],
            },
            Card {
                label: "Screen edges",
                rows: &[
                    r(
                        EDGE_VOLUME,
                        C,
                        "&Volume slider on the right screen edge",
                        "",
                    ),
                    r(
                        EDGE_BRIGHTNESS,
                        C,
                        "&Brightness slider on the left screen edge",
                        "",
                    ),
                ],
            },
            Card {
                label: "Pinned apps",
                rows: &[
                    r(PIN, Kind::Combo(200), "Pinned apps", ""),
                    r(EARLIER, Kind::Btn, "Move pin &earlier", ""),
                    r(LATER, Kind::Btn, "Move pin la&ter", ""),
                ],
            },
        ],
    },
    Page {
        name: "Weather",
        cards: &[
            Card {
                label: "Location",
                rows: &[
                    r(DETECT, C, "Detect location a&utomatically", ""),
                    r(CITY, Kind::Edit(200), "Weather c&ity", "Empty is none"),
                ],
            },
            Card {
                label: "Units",
                rows: &[r(UNITS, Kind::Combo(170), "Temperature units", "")],
            },
        ],
    },
    Page {
        name: "Prayer",
        cards: &[
            Card {
                label: "Reminders",
                rows: &[
                    r(PRAYER, C, "Prayer &reminders", ""),
                    r(PMETHOD, Kind::Combo(230), "Prayer method", ""),
                    r(PLEAD, Kind::Combo(170), "Heads-up before the prayer", ""),
                    r(
                        PPAUSE,
                        C,
                        "Pause &media at prayer time",
                        "Pauses a playing player (Media tab on), never resumes",
                    ),
                ],
            },
            Card {
                label: "Fine-tune",
                rows: &[
                    r(PSHIFT, Kind::Edit(70), "Fajr shift", "Minutes, -30 to 30"),
                    r(
                        PSHIFT + 1,
                        Kind::Edit(70),
                        "Dhuhr shift",
                        "Minutes, -30 to 30",
                    ),
                    r(
                        PSHIFT + 2,
                        Kind::Edit(70),
                        "Asr shift",
                        "Minutes, -30 to 30",
                    ),
                    r(
                        PSHIFT + 3,
                        Kind::Edit(70),
                        "Maghrib shift",
                        "Minutes, -30 to 30",
                    ),
                    r(
                        PSHIFT + 4,
                        Kind::Edit(70),
                        "Isha shift",
                        "Minutes, -30 to 30",
                    ),
                ],
            },
            Card {
                label: "Sound",
                rows: &[
                    r(PSOUND, Kind::Combo(230), "Prayer sound", ""),
                    r(PBROWSE, Kind::Btn, "Bro&wse...", "Choose a sound file"),
                    r(
                        PTEST,
                        Kind::Btn,
                        "Te&st sound",
                        "Plays even when Reminder sounds is off",
                    ),
                ],
            },
        ],
    },
    Page {
        name: "Limits",
        cards: &[
            Card {
                label: "Permissions",
                rows: &[
                    r(LIMCODEX, C, "Run codex on Refresh", ""),
                    r(LIMAGY, C, "Run agy on Refresh", ""),
                ],
            },
            Card {
                label: "Display",
                rows: &[r(
                    LIMGROUP,
                    Kind::Combo(240),
                    "Antigravity limits to show",
                    "",
                )],
            },
            Card {
                label: "Claude Code",
                rows: &[r(
                    LIMSNIP,
                    Kind::Edit(220),
                    "Statusline command",
                    "Optional: copy into settings.json",
                )],
            },
        ],
    },
    Page {
        name: "About",
        cards: &[
            Card {
                label: "Diagnostics",
                rows: &[r(
                    DIAGNOSTICS,
                    Kind::Edit(220),
                    "Dia&gnostics report file",
                    "Empty is off",
                )],
            },
            Card {
                label: "Reset",
                rows: &[r(
                    RESET,
                    Kind::Btn,
                    "&Reset to defaults",
                    "Every setting returns to its default",
                )],
            },
            Card {
                label: "ROQA",
                rows: &[r(
                    LABEL,
                    Kind::Note,
                    concat!("ROQA ", env!("CARGO_PKG_VERSION")),
                    "",
                )],
            },
        ],
    },
];
/// One flat tile colour per sidebar page.
const TILES: [u32; 8] = [
    0x8e8e93, 0xaf52de, 0x0a84ff, 0x30b0c7, 0xff9f0a, 0x30c85e, 0xff453a, 0x636366,
];

struct Item {
    id: usize,
    kind: Kind,
    page: u8,
    card: u8,
    /// The control, its label and its caption: a null handle is absent.
    h: HWND,
    label: HWND,
    cap: HWND,
}

pub struct SettingsWindow {
    window: HWND,
    side: HWND,
    status: HWND,
    close: HWND,
    items: Vec<Item>,
    page: usize,
    move_pin: Option<(usize, bool)>,
    /// The connected monitors' GDI devices (at most `MAX_OFF`); monitor `n` is the checkbox `MONITOR + n`.
    devices: Vec<String>,
    /// The same monitors with their size and place, for the map.
    monitors: Vec<Monitor>,
    /// The audio files found (and the chosen one if it is elsewhere), full paths.
    sounds: Vec<String>,
    sound_action: Option<SoundAction>,
    font: Option<PickerFont>,
    pal: Pal,
    factor: f64,
    scroll: i32,
    scroll_max: i32,
    /// What the host last got; a change is a difference from this.
    current: Settings,
    change: Option<Settings>,
    start: Option<u64>,
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
    sel: u32,
    text: u32,
    dim: u32,
    accent: u32,
    /// Toggle track when off, toggle thumb, the selected sidebar row's text.
    off: u32,
    thumb: u32,
    seltext: u32,
    dark: bool,
}

/// What `WM_ERASEBKGND` draws: the page title, section labels and cards, in client coordinates.
struct Paint {
    pal: Pal,
    scale: f64,
    font: HFONT,
    title_font: HFONT,
    title: (RECT, &'static str),
    labels: Vec<(RECT, &'static str)>,
    /// A card and the y of each divider inside it.
    cards: Vec<(RECT, Vec<i32>)>,
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
/// Windows app mode is dark (read when the window opens and on a settings change, never polled).
fn dark() -> bool {
    let (mut light, mut size) = (1u32, 4u32);
    // SAFETY: reads one DWORD into a local.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut light as *mut u32).cast()),
            Some(&mut size),
        )
    };
    read.is_ok() && light == 0
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

/// The system accent, or the reference green when it cannot be read.
fn accent() -> u32 {
    let (mut colour, mut opaque) = (0u32, BOOL(0));
    // SAFETY: fills two local scalars.
    match unsafe { DwmGetColorizationColor(&mut colour, &mut opaque) } {
        Ok(()) => colour & 0xff_ffff,
        Err(_) => 0x30c85e,
    }
}

/// The tokens of `design-language.md`: dark or light with the system accent, or the system colours in
/// high contrast.
fn palette() -> Pal {
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
            sel: select,
            text,
            dim: system(COLOR_GRAYTEXT),
            accent: select,
            off: system(COLOR_GRAYTEXT),
            thumb: window,
            seltext: system(COLOR_HIGHLIGHTTEXT),
            dark: false,
        };
    }
    let dark = dark();
    let pick = |dark_value: u32, light_value: u32| if dark { dark_value } else { light_value };
    let text = pick(0xf2f2f4, 0x16171a);
    Pal {
        side: pick(0x19191b, 0xececef),
        content: pick(0x1d1d1f, 0xf6f6f8),
        card: pick(0x28282a, 0xffffff),
        line: pick(0x333336, 0xe4e4e8),
        sel: pick(0x2d2d30, 0xffffff),
        text,
        dim: pick(0x8e8e93, 0x6a6b72),
        accent: accent(),
        off: pick(0x48484c, 0xc9c9ce),
        thumb: 0xffffff,
        seltext: text,
        dark,
    }
}

/// A fill in one colour: the stock DC brush and pen carry it, so nothing is created or leaked.
/// `HOLLOW` as the fill draws only the outline.
const HOLLOW: u32 = u32::MAX;
unsafe fn round(dc: HDC, r: RECT, radius: i32, fill: u32, line: u32) {
    // SAFETY: stock objects on the caller's live DC.
    unsafe {
        SetDCBrushColor(dc, rgb(fill));
        SetDCPenColor(dc, rgb(line));
        SelectObject(
            dc,
            GetStockObject(if fill == HOLLOW { NULL_BRUSH } else { DC_BRUSH }),
        );
        SelectObject(dc, GetStockObject(DC_PEN));
        let _ = RoundRect(dc, r.left, r.top, r.right, r.bottom, radius * 2, radius * 2);
    }
}

unsafe fn fill(dc: HDC, r: RECT, colour: u32) {
    // SAFETY: stock brush on the caller's live DC.
    unsafe {
        SetDCBrushColor(dc, rgb(colour));
        FillRect(dc, &r, HBRUSH(GetStockObject(DC_BRUSH).0));
    }
}

unsafe fn write(dc: HDC, text: &str, mut r: RECT, colour: u32, flags: DRAW_TEXT_FORMAT) {
    let mut text: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: draws a local buffer into the caller's live DC.
    unsafe {
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

fn paint(dc: HDC, window: HWND, p: &Paint) {
    let px = |v: i32| (v as f64 * p.scale).round() as i32;
    let mut client = RECT::default();
    // SAFETY: our own live window and the DC Windows gave this message.
    unsafe {
        let _ = GetClientRect(window, &mut client);
        fill(dc, client, p.pal.content);
        fill(
            dc,
            RECT {
                right: px(SIDE_W),
                ..client
            },
            p.pal.side,
        );
        SetBkMode(dc, TRANSPARENT);
        SelectObject(dc, p.title_font.into());
        write(dc, p.title.1, p.title.0, p.pal.text, DT_LEFT | DT_NOPREFIX);
        SelectObject(dc, p.font.into());
        for (rect, text) in &p.labels {
            write(dc, text, *rect, p.pal.dim, DT_LEFT | DT_NOPREFIX);
        }
        for (rect, lines) in &p.cards {
            round(dc, *rect, px(10), p.pal.card, p.pal.line);
            for &y in lines {
                let line = RECT {
                    left: rect.left + px(14),
                    top: y,
                    right: rect.right - px(14),
                    bottom: y + 1,
                };
                fill(dc, line, p.pal.line);
            }
        }
    }
}

/// A sidebar row: the selected one is a lighter rounded fill, each page has a flat coloured tile with its initial.
fn draw_side(item: &DRAWITEMSTRUCT, p: &Paint) {
    let px = |v: i32| (v as f64 * p.scale).round() as i32;
    let Some(page) = PAGES.get(item.itemID as usize) else {
        return;
    };
    let (dc, row) = (item.hDC, item.rcItem);
    let cell = RECT {
        left: row.left + px(8),
        top: row.top + px(2),
        right: row.right - px(8),
        bottom: row.bottom - px(2),
    };
    let tile = RECT {
        left: cell.left + px(8),
        top: cell.top + (cell.bottom - cell.top - px(20)) / 2,
        right: cell.left + px(28),
        bottom: cell.top + (cell.bottom - cell.top - px(20)) / 2 + px(20),
    };
    // SAFETY: the DC and rectangle come from this owner-draw message.
    unsafe {
        fill(dc, row, p.pal.side);
        if item.itemState.0 & ODS_SELECTED.0 != 0 {
            round(dc, cell, px(8), p.pal.sel, p.pal.sel);
        }
        if item.itemState.0 & ODS_FOCUS.0 != 0 {
            round(
                dc,
                cell,
                px(8),
                if item.itemState.0 & ODS_SELECTED.0 != 0 {
                    p.pal.sel
                } else {
                    p.pal.side
                },
                p.pal.accent,
            );
        }
        round(
            dc,
            tile,
            px(5),
            TILES[item.itemID as usize % 8],
            TILES[item.itemID as usize % 8],
        );
        SetBkMode(dc, TRANSPARENT);
        SelectObject(dc, p.font.into());
        let initial = &page.name[..1];
        write(dc, initial, tile, 0xffffff, DT_CENTER | DT_NOPREFIX);
        let name = RECT {
            left: tile.right + px(12),
            right: cell.right,
            ..cell
        };
        let colour = if item.itemState.0 & ODS_SELECTED.0 != 0 {
            p.pal.seltext
        } else {
            p.pal.text
        };
        write(dc, page.name, name, colour, DT_LEFT | DT_NOPREFIX);
    }
}

fn is_toggle(h: HWND) -> bool {
    // SAFETY: scalar query of a live child.
    (unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32) & 0xf == BS_AUTOCHECKBOX as u32
}

/// A checkbox drawn as a switch (the control stays a native checkbox: state, tab stop, mnemonic, UIA role):
/// the title on the left, a pill with a thumb on the right, a two-line accent ring while it has focus.
fn draw_toggle(note: &NMCUSTOMDRAW, p: &Paint) {
    let px = |v: i32| (v as f64 * p.scale).round() as i32;
    let (dc, r, h) = (note.hdc, note.rc, note.hdr.hwndFrom);
    // SAFETY: scalar query of a live child.
    let style = unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32;
    let tall = style & 0xc00 == BS_TOP as u32;
    let on = checked(h);
    let mid = if tall {
        r.top + px(14)
    } else {
        (r.top + r.bottom) / 2
    };
    let pill = RECT {
        left: r.right - px(38),
        top: mid - px(11),
        right: r.right,
        bottom: mid + px(11),
    };
    let thumb = if on {
        pill.right - px(20)
    } else {
        pill.left + px(2)
    };
    let title = RECT {
        right: pill.left - px(8),
        bottom: if tall { r.top + px(28) } else { r.bottom },
        ..r
    };
    // SAFETY: the DC and rectangle come from this custom-draw notification.
    unsafe {
        fill(dc, r, p.pal.card);
        let track = if on { p.pal.accent } else { p.pal.off };
        round(dc, pill, px(11), track, track);
        let knob = RECT {
            left: thumb,
            top: mid - px(9),
            right: thumb + px(18),
            bottom: mid + px(9),
        };
        round(dc, knob, px(9), p.pal.thumb, p.pal.thumb);
        SetBkMode(dc, TRANSPARENT);
        SelectObject(dc, p.font.into());
        write(dc, &text(h, 256), title, p.pal.text, DT_LEFT);
        if note.uItemState.0 & CDIS_FOCUS.0 != 0 {
            round(dc, r, px(8), HOLLOW, p.pal.accent);
            let inner = RECT {
                left: r.left + 1,
                top: r.top + 1,
                right: r.right - 1,
                bottom: r.bottom - 1,
            };
            round(dc, inner, px(8), HOLLOW, p.pal.accent);
        }
    }
}

fn is_tile(h: HWND) -> bool {
    // SAFETY: scalar query of a live child.
    (MONITOR..MONITOR + MAX_OFF).contains(&(unsafe { GetDlgCtrlID(h) } as usize))
}

/// A monitor tile (a native checkbox underneath): lit with an accent ring when ROQA is on there, dim when off.
/// It reads its lines from the checkbox text, "Display 1, primary, 1920 by 1080 at 125%, ROQA on".
fn draw_tile(note: &NMCUSTOMDRAW, p: &Paint) {
    let px = |v: i32| (v as f64 * p.scale).round() as i32;
    let (dc, r, h) = (note.hdc, note.rc, note.hdr.hwndFrom);
    let on = checked(h);
    let label = text(h, 256);
    let parts: Vec<&str> = label.split(", ").collect();
    let (line, rest) = parts.split_at(parts.len().saturating_sub(2));
    let head = if line.len() > 1 {
        format!("{} ({})", line[0], line[1])
    } else {
        line.join(" ")
    };
    let (fill_colour, ink) = if on {
        (p.pal.sel, p.pal.seltext)
    } else {
        (p.pal.side, p.pal.dim)
    };
    // SAFETY: the DC and rectangle come from this custom-draw notification.
    unsafe {
        fill(dc, r, p.pal.card);
        round(
            dc,
            r,
            px(8),
            fill_colour,
            if on { p.pal.accent } else { p.pal.line },
        );
        SetBkMode(dc, TRANSPARENT);
        SelectObject(dc, p.font.into());
        let lines = [head.as_str(), rest[0], rest[1]];
        for (n, line) in lines.into_iter().enumerate() {
            let top = r.top + px(6) + n as i32 * px(17);
            if top + px(17) <= r.bottom {
                let at = RECT {
                    left: r.left + px(8),
                    top,
                    right: r.right - px(6),
                    bottom: top + px(17),
                };
                write(dc, line, at, ink, DT_LEFT | DT_NOPREFIX | DT_END_ELLIPSIS);
            }
        }
        if note.uItemState.0 & CDIS_FOCUS.0 != 0 {
            let inner = RECT {
                left: r.left + 2,
                top: r.top + 2,
                right: r.right - 2,
                bottom: r.bottom - 2,
            };
            round(dc, r, px(8), HOLLOW, p.pal.accent);
            round(dc, inner, px(7), HOLLOW, p.pal.accent);
        }
    }
}

/// Dialog procedure: the settings colours, background and sidebar rows; everything else is the shared one.
unsafe extern "system" fn settings_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    let handled = PAINT.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let p = guard.as_ref()?;
        match message {
            WM_ERASEBKGND => {
                paint(HDC(wparam.0 as *mut _), hwnd, p);
                Some(1)
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                let dc = HDC(wparam.0 as *mut _);
                // SAFETY: the DC is the one this message carries; the child is live.
                unsafe {
                    let (text, back) = match GetDlgCtrlID(HWND(lparam.0 as *mut _)) as usize {
                        STATUS => (p.pal.dim, p.pal.content),
                        SIDE => (p.pal.text, p.pal.side),
                        CAPTION => (p.pal.dim, p.pal.card),
                        _ => (p.pal.text, p.pal.card),
                    };
                    SetTextColor(dc, rgb(text));
                    SetBkColor(dc, rgb(back));
                    SetDCBrushColor(dc, rgb(back));
                    Some(GetStockObject(DC_BRUSH).0 as isize)
                }
            }
            WM_NOTIFY => {
                // SAFETY: for WM_NOTIFY lparam is an NMHDR that lives through this call; only a custom-draw
                // notification (checked first) is an NMCUSTOMDRAW.
                if unsafe { (*(lparam.0 as *const NMHDR)).code } != NM_CUSTOMDRAW {
                    return None;
                }
                // SAFETY: the code says this is an NMCUSTOMDRAW.
                let note = unsafe { &*(lparam.0 as *const NMCUSTOMDRAW) };
                if note.dwDrawStage != CDDS_PREPAINT || !is_toggle(note.hdr.hwndFrom) {
                    return None;
                }
                if is_tile(note.hdr.hwndFrom) {
                    draw_tile(note, p);
                } else {
                    draw_toggle(note, p);
                }
                // SAFETY: the result of this notification (DWLP_MSGRESULT, offset 0), on our own dialog.
                unsafe {
                    SetWindowLongPtrW(hwnd, WINDOW_LONG_PTR_INDEX(0), CDRF_SKIPDEFAULT as isize)
                };
                Some(1)
            }
            WM_DRAWITEM => {
                // SAFETY: for WM_DRAWITEM lparam is a DRAWITEMSTRUCT that lives through this call.
                draw_side(unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) }, p);
                Some(1)
            }
            _ => None,
        }
    });
    match handled {
        Some(result) => result,
        // SAFETY: forwards the same arguments to the shared dialog procedure.
        None => unsafe { dialog_proc(hwnd, message, wparam, lparam) },
    }
}

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
        DETECT => &mut s.detect,
        PRAYER => &mut s.prayer_remind,
        LIMCODEX => &mut s.limits_codex,
        LIMAGY => &mut s.limits_agy,
        FSHIDE => &mut s.hide_fullscreen,
        RSOUNDS => &mut s.reminder_sounds,
        PPAUSE => &mut s.prayer_pause,
        TAB..=159 if id - TAB < 6 => &mut s.tabs[id - TAB],
        DBTN..=134 => &mut s.dock_buttons.shown[id - DBTN],
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
/// Adds the rows of the drop-down list that table row `id` names (the pin and sound lists are filled elsewhere).
fn fill_combo(h: HWND, id: usize) {
    let rows: &[&str] = match id {
        THEME => return Theme::ALL.iter().for_each(|t| add_row(h, t.label())),
        DOCKSTYLE => return DockStyle::ALL.iter().for_each(|s| add_row(h, s.label())),
        UNITS => return Units::ALL.iter().for_each(|u| add_row(h, u.label())),
        PMETHOD => return Method::ALL.iter().for_each(|m| add_row(h, m.label())),
        PLEAD => &LEAD_ROWS,
        LIMGROUP => &["Gemini models", "Claude and GPT models"],
        NSIZE => &["Auto", "115%", "130%", "150%"],
        NOPEN => &["Hover", "Click"],
        ANIM => &["Normal", "Fast"],
        _ => &[],
    };
    rows.iter().for_each(|row| add_row(h, row));
}

impl SettingsWindow {
    pub fn create(current: &Settings, monitors: &[Monitor], factor: f64) -> Result<Self> {
        let window = empty_dialog(480, 300, Some(settings_proc))?;
        if crate::dock::system_rtl() {
            // Children created after this inherit the mirrored layout.
            // SAFETY: our own dialog, before any child exists.
            unsafe {
                let ex = GetWindowLongPtrW(window, GWL_EXSTYLE);
                SetWindowLongPtrW(window, GWL_EXSTYLE, ex | WS_EX_LAYOUTRTL.0 as isize);
            }
        }
        let mut this = Self {
            window,
            side: HWND::default(),
            status: HWND::default(),
            close: HWND::default(),
            items: Vec::new(),
            page: 0,
            move_pin: None,
            devices: Vec::new(),
            monitors: Vec::new(),
            sounds: prayer_os::discover(),
            sound_action: None,
            font: None,
            pal: palette(),
            factor,
            scroll: 0,
            scroll_max: 0,
            current: current.clone(),
            change: None,
            start: None,
            return_to: foreground_key(),
            closed: false,
        };
        this.set_monitors(monitors);
        this.build()?;
        this.fill_sounds(&current.prayer_sound);
        this.show(current);
        this.relayout()?;
        // SAFETY: explicit user request only. Windows retains its normal foreground restrictions.
        unsafe {
            SetWindowTextW(window, w!("ROQA settings"))?;
            let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            if SetForegroundWindow(window).as_bool() {
                SetFocus(Some(this.side))?;
            }
        }
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

    pub fn set_pins(&self, names: &[String], selected: usize) {
        let pins = self.h(PIN);
        // SAFETY: synchronous string copies to our own native combo box.
        unsafe {
            SendMessageW(pins, CB_RESETCONTENT, None, None);
        }
        for name in names {
            add_row(pins, name);
        }
        select(pins, Some(clamp_selection(selected as isize, names.len())));
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

    /// One control row: the label before its control and the caption after it keep the tab and reading order.
    fn add(
        &self,
        id: usize,
        kind: Kind,
        title: &str,
        caption: &str,
        page: usize,
        card: usize,
    ) -> Result<Item> {
        let window = self.window;
        let (tab, clip, flat) = (WS_TABSTOP.0, WS_CLIPSIBLINGS.0, WINDOW_EX_STYLE(0));
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
                BS_AUTOCHECKBOX as u32
                    | tab
                    | clip
                    | if tall { BS_TOP as u32 } else { 0 }
                    // Arrow keys move among the monitor tiles only.
                    | if matches!(id, MONITOR | DESKTOP) { WS_GROUP.0 } else { 0 },
                flat,
            )?,
            Kind::Combo(_) => {
                label = stat(title, LABEL)?;
                control(
                    window,
                    w!("ComboBox"),
                    "",
                    id,
                    CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0 | tab | clip,
                    flat,
                )?
            }
            Kind::Edit(_) => {
                label = stat(title, LABEL)?;
                let extra = match id {
                    SHELF => ES_NUMBER as u32,
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
                    ES_AUTOHSCROLL as u32 | WS_BORDER.0 | tab | clip | extra,
                    WS_EX_CLIENTEDGE,
                )?
            }
            Kind::Btn => {
                if !caption.is_empty() {
                    label = stat(caption, LABEL)?;
                }
                control(window, w!("Button"), title, id, tab | clip, flat)?
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
            PSHIFT..=174 => 4,
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
        })
    }

    /// The tile's accessible name (and what it draws): "Display 1, primary, 1920 by 1080 at 125%, ROQA on".
    fn tile_name(&self, index: usize, on: bool) -> String {
        let m = &self.monitors[index];
        format!(
            "Display {}{}, {} by {} at {}%, ROQA {}",
            index + 1,
            if m.rect.x == 0 && m.rect.y == 0 {
                ", primary"
            } else {
                ""
            },
            m.rect.width,
            m.rect.height,
            m.dpi * 100 / 96,
            if on { "on" } else { "off" }
        )
    }

    fn monitor_item(&self, index: usize) -> Result<Item> {
        let on = !self.current.off_monitors.contains(&self.devices[index]);
        let item = self.add(
            MONITOR + index,
            Kind::Check,
            &self.tile_name(index, on),
            "",
            0,
            MAP_CARD,
        )?;
        set_checked(item.h, on);
        Ok(item)
    }

    /// Replaces the monitor checkboxes after the monitor list changed; focus stays in a live control.
    pub fn refresh_monitors(&mut self, monitors: &[Monitor]) -> Result<()> {
        if self.monitors.iter().eq(monitors.iter().take(MAX_OFF)) {
            return Ok(());
        }
        self.set_monitors(monitors);
        // SAFETY: own controls and current thread focus; no foreground activation.
        let focus = unsafe { GetFocus() };
        let is_monitor = |i: &Item| (MONITOR..MONITOR + MAX_OFF).contains(&i.id);
        if self.items.iter().any(|i| is_monitor(i) && i.h == focus) {
            // SAFETY: our own sidebar list.
            unsafe { SetFocus(Some(self.side))? };
        }
        for item in self.items.iter().filter(|i| is_monitor(i)) {
            // SAFETY: owned monitor checkbox, no retained references.
            unsafe { DestroyWindow(item.h)? };
        }
        self.items.retain(|i| !is_monitor(i));
        // Tab order follows the screen: each new checkbox goes right after the previous one.
        let at = self
            .items
            .iter()
            .position(|i| i.id == NOTCH)
            .map_or(0, |i| i + 1);
        let mut previous = self
            .items
            .get(at.wrapping_sub(1))
            .map_or(HWND::default(), |i| i.h);
        for index in 0..self.devices.len() {
            let item = self.monitor_item(index)?;
            // SAFETY: both are our own live children; this only reorders them, no move, size or activation.
            unsafe {
                SetWindowPos(
                    item.h,
                    Some(previous),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )?;
                if self.page != 0 {
                    let _ = ShowWindow(item.h, SW_HIDE);
                }
            }
            previous = item.h;
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
        self.layout()
    }

    /// Creates the controls in tab order: the sidebar, each page's rows top to bottom, then the footer.
    fn build(&mut self) -> Result<()> {
        let window = self.window;
        self.side = control(
            window,
            w!("ListBox"),
            "",
            SIDE,
            (LBS_NOTIFY | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT) as u32
                | WS_TABSTOP.0,
            WINDOW_EX_STYLE(0),
        )?;
        for page in PAGES {
            let name = wide(page.name);
            // SAFETY: the list box copies the terminated string synchronously.
            unsafe {
                SendMessageW(
                    self.side,
                    LB_ADDSTRING,
                    None,
                    Some(LPARAM(name.as_ptr() as isize)),
                );
            }
        }
        // SAFETY: scalar selection on our own list box.
        unsafe {
            SendMessageW(self.side, LB_SETCURSEL, Some(WPARAM(0)), None);
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
            "",
            STATUS,
            SS_CENTERIMAGE_NOTIFY | WS_CLIPSIBLINGS.0,
            WS_EX_NOPARENTNOTIFY,
        )?;
        self.close = control(
            window,
            w!("Button"),
            "&Close",
            CLOSE,
            WS_TABSTOP.0,
            WINDOW_EX_STYLE(0),
        )?;
        for item in &self.items {
            if item.page != 0 {
                for h in [item.h, item.label, item.cap] {
                    if !h.0.is_null() {
                        // SAFETY: our own live child.
                        let _ = unsafe { ShowWindow(h, SW_HIDE) };
                    }
                }
            }
        }
        Ok(())
    }

    /// Shows page `page`: only its controls are visible (hidden ones leave the tab order), scrolled to the top.
    fn show_page(&mut self, page: usize) -> Result<()> {
        self.page = page.min(PAGES.len() - 1);
        self.scroll = 0;
        for item in &self.items {
            let command = if item.page as usize == self.page {
                SW_SHOWNA
            } else {
                SW_HIDE
            };
            for h in [item.h, item.label, item.cap] {
                if !h.0.is_null() {
                    // SAFETY: our own live child.
                    let _ = unsafe { ShowWindow(h, command) };
                }
            }
        }
        self.layout()?;
        // SAFETY: our own live window.
        let _ = unsafe { InvalidateRect(Some(self.window), None, true) };
        Ok(())
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
                        let _ = set_text(h, &self.tile_name(item.id - MONITOR, on));
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
                        NSIZE => row_of(&NOTCH_SIZES, &settings.notch_size),
                        NOPEN => usize::from(settings.open_click),
                        ANIM => usize::from(settings.anim_fast),
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
            add_row(combo, prayer_tab::label(sound));
        }
    }

    /// The host stored a file the user browsed to: list it and select it.
    pub fn set_sound(&mut self, setting: &str) {
        self.current.prayer_sound = setting.to_owned();
        self.fill_sounds(setting);
        select(self.h(PSOUND), Some(self.sound_row(setting)));
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
                        THEME => pick(&Theme::ALL, &mut next.theme, row),
                        DOCKSTYLE => pick(&DockStyle::ALL, &mut next.dock_style, row),
                        UNITS => pick(&Units::ALL, &mut next.units, row),
                        PMETHOD => pick(&Method::ALL, &mut next.prayer_method, row),
                        PLEAD => pick(&LEADS, &mut next.prayer_lead, row),
                        NSIZE => pick(&NOTCH_SIZES, &mut next.notch_size, row),
                        NOPEN => next.open_click = row.map_or(next.open_click, |r| r == 1),
                        ANIM => next.anim_fast = row.map_or(next.anim_fast, |r| r == 1),
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
        let next = ranged.clone().keep_one_monitor(&self.devices);
        self.show(&next);
        if invalid {
            // The box already shows the value that was kept (`show` above), so the message explains the change.
            self.say("A prayer shift is a whole number of minutes from -30 to 30. The nearest allowed value is kept.")?;
        } else if !kept_a_kind {
            self.say("Keep the dock or the notch on.")?;
        } else if ranged != next {
            self.say("Keep at least one monitor on.")?;
        } else if next != self.current {
            self.say("Applied.")?;
        }
        if next != self.current {
            self.current = next.clone();
            self.change = Some(next);
        }
        Ok(())
    }

    pub fn say(&self, status: &str) -> Result<()> {
        set_text(self.status, status)
    }

    /// The newest settings the user chose since the host last asked.
    pub fn take_change(&mut self) -> Option<Settings> {
        self.change.take()
    }

    /// A timer length in seconds the user asked to start.
    pub fn take_start(&mut self) -> Option<u64> {
        self.start.take()
    }

    pub fn message(&mut self, message: &MSG) -> Result<bool> {
        if message.hwnd != self.window {
            return Ok(false);
        }
        match message.message {
            ACTION => {
                let id = message.wParam.0 & 0xffff;
                let notification = (message.wParam.0 >> 16) as u32;
                let kind = self.items.iter().find(|i| i.id == id).map(|i| i.kind);
                match (id, kind) {
                    (SIDE, _) if notification == LBN_SELCHANGE => {
                        // SAFETY: scalar query of our own list box.
                        let row = unsafe { SendMessageW(self.side, LB_GETCURSEL, None, None) }.0;
                        self.show_page(row.max(0) as usize)?;
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
                    (START, _) if notification == BN_CLICKED => {
                        match parse_timer(&text(self.h(TIMER), 8)) {
                            Some(seconds) => self.start = Some(seconds),
                            None => self.say(
                                "Enter minutes, m:ss or h:mm:ss, from 1 second to 24 hours.",
                            )?,
                        }
                    }
                    (RESET, _) if notification == BN_CLICKED => {
                        self.show(&Settings::default());
                        self.commit()?;
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
            SCROLL => self.scroll_message(message.wParam.0 & 0xffff)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    pub fn keyboard(&self, message: &MSG) -> bool {
        // Enter toggles a focused monitor tile like Space does (it would otherwise press the default button).
        // SAFETY: scalar queries and a click on our own live tile.
        unsafe {
            if message.message == WM_KEYDOWN
                && message.wParam.0 == 0x0d
                && IsChild(self.window, message.hwnd).as_bool()
                && is_tile(message.hwnd)
            {
                SendMessageW(message.hwnd, BM_CLICK, None, None);
                return true;
            }
        }
        // SAFETY: no Rust state is exposed to the reentrant dialog procedure; it posts scalar actions only.
        unsafe {
            let handled = IsDialogMessageW(self.window, message).as_bool();
            if handled {
                let _ = PostMessageW(Some(self.window), SCROLL, WPARAM(9), LPARAM(0));
            }
            handled
        }
    }

    fn scroll_message(&mut self, action: usize) -> Result<()> {
        let mut scroll = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_ALL,
            ..Default::default()
        };
        // SAFETY: queries our own scroll bar and focus rectangle; no callbacks into Rust state.
        let next = unsafe {
            GetScrollInfo(self.window, SB_VERT, &mut scroll)?;
            match action as i32 {
                0 => self.scroll - 48,
                1 => self.scroll + 48,
                2 => self.scroll - scroll.nPage as i32,
                3 => self.scroll + scroll.nPage as i32,
                4 | 5 => scroll.nTrackPos,
                6 => 0,
                7 => self.scroll_max,
                9 => {
                    let focus = GetFocus();
                    let mut origin = POINT::default();
                    let mut rect = RECT::default();
                    if !IsChild(self.window, focus).as_bool()
                        || focus == self.side
                        || !ClientToScreen(self.window, &mut origin).as_bool()
                    {
                        return Ok(());
                    }
                    GetWindowRect(focus, &mut rect)?;
                    focus_scroll(
                        self.scroll,
                        rect.top - origin.y,
                        rect.bottom - origin.y,
                        scroll.nPage as i32,
                    )
                }
                _ => return Ok(()),
            }
        }
        .clamp(0, self.scroll_max);
        if next != self.scroll {
            self.scroll = next;
            self.layout()?;
            // The title, cards and dividers are painted by the parent, not moved with the children.
            // SAFETY: our own live window.
            let _ = unsafe { InvalidateRect(Some(self.window), None, true) };
        }
        Ok(())
    }

    /// The Windows text size changed.
    pub fn set_factor(&mut self, factor: f64) -> Result<()> {
        self.factor = factor;
        self.relayout()
    }

    /// New font (DPI or text size changed): applies it to every control, sizes the window, then lays out.
    fn relayout(&mut self) -> Result<()> {
        let work = native_window::primary_work_area()?;
        // SAFETY: owned dialog is live.
        let dpi = unsafe { GetDpiForWindow(self.window) }.max(96);
        let font = PickerFont::create(dpi, self.factor)?;
        self.pal = palette();
        let dark = BOOL::from(self.pal.dark);
        // SAFETY: a scalar attribute on our own window; older Windows ignores it.
        unsafe {
            let _ = DwmSetWindowAttribute(
                self.window,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&dark as *const BOOL).cast(),
                size_of::<BOOL>() as u32,
            );
        }
        let handles = self
            .items
            .iter()
            .flat_map(|i| [i.h, i.label, i.cap])
            .chain([self.side, self.status, self.close])
            .filter(|h| !h.0.is_null());
        for child in handles {
            // The native look of lists, edits and buttons follows the app mode too.
            // SAFETY: our own live child.
            unsafe {
                let _ = SetWindowTheme(
                    child,
                    if self.pal.dark {
                        w!("DarkMode_Explorer")
                    } else {
                        PCWSTR::null()
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
        let px = |v: i32| (v as f64 * scale).round() as i32;
        let mut frame = RECT {
            right: px(654),
            bottom: px(556),
            ..Default::default()
        };
        // SAFETY: fills the local RECT for the dialog's fixed style at this DPI; positions only our own window.
        unsafe {
            AdjustWindowRectExForDpi(
                &mut frame,
                WINDOW_STYLE(DIALOG_STYLE | WS_VSCROLL.0),
                false,
                WINDOW_EX_STYLE(0),
                dpi,
            )?;
            let width = (frame.right - frame.left).min(work.width);
            let height = (frame.bottom - frame.top).min(work.height);
            let style = GetWindowLongPtrW(self.window, GWL_STYLE);
            SetWindowLongPtrW(
                self.window,
                GWL_STYLE,
                style | (WS_VSCROLL.0 | WS_CLIPCHILDREN.0) as isize,
            );
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
        self.layout()?;
        // SAFETY: our own live window.
        let _ = unsafe { InvalidateRect(Some(self.window), None, true) };
        Ok(())
    }

    /// Places the sidebar, the current page's rows and the footer, and hands the painter what to draw.
    fn layout(&mut self) -> Result<()> {
        let Some(font) = self.font.as_ref() else {
            return Ok(());
        };
        let (handle, scale) = (font.handle, font.scale);
        let px = |v: i32| (v as f64 * scale).round() as i32;
        let mut client = RECT::default();
        // SAFETY: our own live window.
        unsafe { GetClientRect(self.window, &mut client)? };
        let (mut geometry, content) = self.place(scale, client)?;
        let view = (client.bottom - px(FOOT)).max(1);
        self.scroll_max = (content - view).max(0);
        let clamped = self.scroll.clamp(0, self.scroll_max);
        let info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: content - 1,
            nPage: view as u32,
            nPos: clamped,
            ..Default::default()
        };
        // SAFETY: our own live window and children.
        unsafe {
            SetScrollInfo(self.window, SB_VERT, &info, true);
            let before = client.right;
            GetClientRect(self.window, &mut client)?;
            // A scroll bar appearing or a clamped position changes the geometry once more.
            if client.right != before || clamped != self.scroll {
                self.scroll = clamped;
                geometry = self.place(scale, client)?.0;
            }
            let side = px(SIDE_W);
            let list = &[
                (self.side, 0, px(10), side, client.bottom - px(10)),
                (
                    self.status,
                    side,
                    client.bottom - px(FOOT),
                    client.right - side,
                    px(FOOT),
                ),
                (
                    self.close,
                    client.right - px(GUTTER + 100),
                    client.bottom - px(FOOT) + px(8),
                    px(100),
                    px(28),
                ),
            ];
            for &(h, x, y, w, ht) in list {
                SetWindowPos(h, None, x, y, w, ht, SWP_NOZORDER | SWP_NOACTIVATE)?;
            }
            SendMessageW(
                self.side,
                LB_SETITEMHEIGHT,
                Some(WPARAM(0)),
                Some(LPARAM(px(36) as isize)),
            );
            // The title font is made once per body font (a scroll step reuses it).
            let old = PAINT.with(|p| p.take());
            let title_font = match &old {
                Some(old) if old.font == handle => old.title_font,
                _ => {
                    if let Some(old) = &old {
                        let _ = DeleteObject(old.title_font.into());
                    }
                    let mut face = LOGFONTW::default();
                    GetObjectW(
                        handle.into(),
                        size_of::<LOGFONTW>() as i32,
                        Some((&mut face as *mut LOGFONTW).cast()),
                    );
                    face.lfHeight = face.lfHeight * 3 / 2;
                    face.lfWeight = 600;
                    CreateFontIndirectW(&face)
                }
            };
            PAINT.with(|p| {
                p.replace(Some(Paint {
                    pal: self.pal,
                    scale,
                    font: handle,
                    title_font,
                    title: geometry.title,
                    labels: geometry.labels,
                    cards: geometry.cards,
                }))
            });
        }
        Ok(())
    }

    /// Moves the current page's controls (top to bottom, scrolled) and returns the geometry and content height.
    fn place(&self, scale: f64, client: RECT) -> Result<(Geometry, i32)> {
        let px = |v: i32| (v as f64 * scale).round() as i32;
        let (left, right) = (
            px(SIDE_W + GUTTER),
            (client.right - px(GUTTER)).max(px(SIDE_W + GUTTER + 100)),
        );
        let mut geometry = Geometry {
            title: (RECT::default(), PAGES[self.page].name),
            labels: Vec::new(),
            cards: Vec::new(),
        };
        let mut y = px(GUTTER) - self.scroll;
        geometry.title.0 = RECT {
            left,
            top: y,
            right,
            bottom: y + px(30),
        };
        y += px(30 + 14);
        for (index, card) in PAGES[self.page].cards.iter().enumerate() {
            geometry.labels.push((
                RECT {
                    left,
                    top: y,
                    right,
                    bottom: y + px(16),
                },
                card.label,
            ));
            y += px(16 + 6);
            let (top, mut lines) = (y, Vec::new());
            let rows = self
                .items
                .iter()
                .filter(|i| i.page as usize == self.page && i.card as usize == index)
                .filter(|i| !(MONITOR..MONITOR + MAX_OFF).contains(&i.id));
            for (n, item) in rows.enumerate() {
                if n > 0 {
                    lines.push(y);
                }
                let tall = !item.cap.0.is_null();
                let height = px(if tall { 52 } else { 40 });
                self.row(item, scale, (left + px(14), right - px(14)), y, height)?;
                y += height;
            }
            if self.page == 0 && index == MAP_CARD {
                y += self.map(scale, (left + px(14), right - px(14)), y)?;
            }
            geometry.cards.push((
                RECT {
                    left,
                    top,
                    right,
                    bottom: y,
                },
                lines,
            ));
            y += px(14);
        }
        Ok((geometry, y + self.scroll + px(8)))
    }

    /// Places the monitor tiles to scale from `y` down and returns the map's height.
    fn map(&self, scale: f64, (a, b): (i32, i32), y: i32) -> Result<i32> {
        let px = |v: i32| (v as f64 * scale).round() as i32;
        let rects: Vec<_> = self.monitors.iter().map(|m| m.rect).collect();
        let tiles = topology::fit(&rects, b - a, px(MAP_H), px(3));
        for (index, tile) in tiles.iter().enumerate() {
            // SAFETY: our own live tile.
            unsafe {
                SetWindowPos(
                    self.h(MONITOR + index),
                    None,
                    a + tile.x,
                    y + px(10) + tile.y,
                    tile.width,
                    tile.height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )?;
            }
        }
        Ok(px(MAP_H + 20))
    }

    fn row(&self, item: &Item, scale: f64, (a, b): (i32, i32), y: i32, height: i32) -> Result<()> {
        let px = |v: i32| (v as f64 * scale).round() as i32;
        let centre = |h: i32| y + (height - h) / 2;
        let tall = !item.cap.0.is_null();
        let put = |h: HWND, x: i32, y: i32, w: i32, ht: i32| {
            if h.0.is_null() {
                return Ok(());
            }
            // SAFETY: our own live child.
            unsafe { SetWindowPos(h, None, x, y, w.max(1), ht, SWP_NOZORDER | SWP_NOACTIVATE) }
        };
        match item.kind {
            Kind::Check => {
                let top = if tall { px(6) } else { 0 };
                put(item.h, a, y + top, b - a, height - top)?;
                put(item.cap, a + px(22), y + px(30), b - a - px(22), px(18))
            }
            Kind::Combo(w) | Kind::Edit(w) => {
                let w = px(w).min((b - a) / 2);
                let drop = if matches!(item.kind, Kind::Combo(_)) {
                    px(180)
                } else {
                    px(28)
                };
                put(item.h, b - w, centre(px(28)), w, drop)?;
                let top = if tall { y + px(8) } else { centre(px(20)) };
                put(item.label, a, top, b - a - w - px(8), px(20))?;
                put(item.cap, a, y + px(28), b - a - w - px(8), px(18))
            }
            Kind::Btn => {
                put(item.h, b - px(170), centre(px(28)), px(170), px(28))?;
                put(item.label, a, centre(px(20)), b - a - px(178), px(20))
            }
            Kind::Note => put(item.label, a, centre(px(20)), b - a, px(20)),
        }
    }
}

struct Geometry {
    title: (RECT, &'static str),
    labels: Vec<(RECT, &'static str)>,
    cards: Vec<(RECT, Vec<i32>)>,
}

impl Drop for SettingsWindow {
    fn drop(&mut self) {
        if let Some(paint) = PAINT.with(|p| p.take()) {
            // SAFETY: the font is ours alone and the window that used it is destroyed next.
            let _ = unsafe { DeleteObject(paint.title_font.into()) };
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

fn focus_scroll(scroll: i32, top: i32, bottom: i32, viewport: i32) -> i32 {
    if top < 0 {
        scroll + top
    } else if bottom > viewport {
        scroll + bottom - viewport
    } else {
        scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dock::Button, tabs::Tab};

    #[test]
    fn keyboard_focus_scrolls_both_ends_into_the_viewport() {
        assert_eq!(focus_scroll(100, -20, 4, 600), 80);
        assert_eq!(focus_scroll(100, 590, 618, 600), 118);
        assert_eq!(focus_scroll(100, 12, 40, 600), 100);
        assert_eq!(focus_scroll(0, 1190, 1218, 600), 618);
    }

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
        assert_eq!(PAGES.len(), TILES.len());
    }

    #[test]
    fn every_tab_button_and_toggle_is_a_row_with_a_setting() {
        let ids: Vec<usize> = rows().iter().map(|r| r.id).collect();
        let mut s = Settings::default();
        for tab in Tab::ALL {
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
}
