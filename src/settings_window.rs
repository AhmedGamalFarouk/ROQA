//! The settings window (M5 S7): a modeless standard-control window in the `picker.rs` style, created on
//! demand and destroyed on close. Standard checkboxes, edits and buttons give tab order, mnemonics, Narrator
//! and text scaling by default. It owns no timer or thread: it exists only while open and does work only in
//! the message loop turns its own controls cause. A change is handed to the host (`take_change`) which applies
//! it live; a timer request likewise (`take_start`).

use crate::{
    dock::{foreground_key, restore_foreground},
    layout::Bounds,
    layout::DockStyle,
    native_window,
    picker::{ACTION, CLOSE, DIALOG_STYLE, RELAYOUT, control, empty_dialog, wide},
    picker_font::PickerFont,
    prayer::Method,
    prayer_os,
    prayer_tab::{self, LEADS, SILENT},
    settings::{MAX_OFF, MAX_PATH, Settings, parse_timer},
    shelf,
    tabs::Tab,
    theme::Theme,
    weather::{MAX_CITY, Units},
    window_model::WindowKey,
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HWND, LPARAM, RECT, WPARAM},
        UI::{
            Controls::*,
            HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
            Input::KeyboardAndMouse::SetFocus,
            WindowsAndMessaging::*,
        },
    },
    core::{Error, PCWSTR, Result, w},
};

const PIN: usize = 140;
const EARLIER: usize = 141;
const LATER: usize = 142;
const DOCK: usize = 100;
const NOTCH: usize = 101;
const SOUND: usize = 102;
const DESKTOP: usize = 111;
const EDGE_VOLUME: usize = 112;
const EDGE_BRIGHTNESS: usize = 113;
/// One checkbox per notch tab (`Tab::ALL` order), then the mascot eyes.
const TAB: usize = 150;
const EYES: usize = TAB + 5;
/// The weather city, the device-location checkbox and the units list.
const CITY: usize = 114;
const DETECT: usize = 115;
const UNITS: usize = 116;
const DOCKSTYLE: usize = 117;
/// The prayer reminder checkbox, method list, sound list with Browse and Test, and the heads-up list.
const PRAYER: usize = 160;
const PMETHOD: usize = 161;
const PSOUND: usize = 162;
const PBROWSE: usize = 163;
const PTEST: usize = 164;
const PLEAD: usize = 165;
const SHELF: usize = 103;
const THEME: usize = 110;
const DIAGNOSTICS: usize = 104;
const TIMER: usize = 105;
const START: usize = 106;
const STATUS: usize = 107;
const RESET: usize = 108;
const LABEL: usize = 109;
const MONITOR: usize = 120;
const OK: usize = 1;
/// Timer length shown when the window opens.
const FIRST_TIMER: &str = "5";

/// What the Settings window asks the host to do with the prayer sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundAction {
    Browse,
    Test,
}

pub struct SettingsWindow {
    window: HWND,
    pin_label: HWND,
    pins: HWND,
    earlier: HWND,
    later: HWND,
    move_pin: Option<(usize, bool)>,
    dock: HWND,
    notch: HWND,
    sound: HWND,
    /// "Show only windows on this desktop".
    desktop: HWND,
    /// The volume (right) and brightness (left) edge sliders.
    edge_volume: HWND,
    edge_brightness: HWND,
    /// "Media tab" and so on, in `Tab::ALL` order, and "Mascot eyes".
    tabs: [HWND; 5],
    eyes: HWND,
    city: HWND,
    detect: HWND,
    /// Drop-down list of `Units::ALL`, in that order.
    units: HWND,
    /// Drop-down list of `DockStyle::ALL`.
    dock_style: HWND,
    prayer: HWND,
    /// Drop-down list of `Method::ALL`.
    pmethod: HWND,
    /// Drop-down list: row 0 is silent, then `sounds` in order.
    psound: HWND,
    pbrowse: HWND,
    ptest: HWND,
    /// Drop-down list of `LEADS`.
    plead: HWND,
    /// The audio files found (and the chosen one if it is elsewhere), full paths.
    sounds: Vec<String>,
    sound_action: Option<SoundAction>,
    /// Drop-down list of `Theme::ALL`, in that order.
    theme: HWND,
    /// One checkbox per connected monitor (at most `MAX_OFF`), named by its GDI device.
    monitors: Vec<(HWND, String)>,
    shelf: HWND,
    diagnostics: HWND,
    timer: HWND,
    status: HWND,
    /// Every other child, in tab order, for the font and the layout.
    others: Vec<HWND>,
    font: Option<PickerFont>,
    factor: f64,
    /// What the host last got; a change is a difference from this.
    current: Settings,
    change: Option<Settings>,
    start: Option<u64>,
    return_to: Option<WindowKey>,
    closed: bool,
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

impl SettingsWindow {
    pub fn create(current: &Settings, devices: &[String], factor: f64) -> Result<Self> {
        let window = empty_dialog(480, 300)?;
        let mut this = Self {
            window,
            pin_label: HWND::default(),
            pins: HWND::default(),
            earlier: HWND::default(),
            later: HWND::default(),
            move_pin: None,
            dock: HWND::default(),
            notch: HWND::default(),
            sound: HWND::default(),
            desktop: HWND::default(),
            edge_volume: HWND::default(),
            edge_brightness: HWND::default(),
            tabs: [HWND::default(); 5],
            eyes: HWND::default(),
            city: HWND::default(),
            detect: HWND::default(),
            units: HWND::default(),
            dock_style: HWND::default(),
            prayer: HWND::default(),
            pmethod: HWND::default(),
            psound: HWND::default(),
            pbrowse: HWND::default(),
            ptest: HWND::default(),
            plead: HWND::default(),
            sounds: prayer_os::discover(),
            sound_action: None,
            theme: HWND::default(),
            monitors: Vec::new(),
            shelf: HWND::default(),
            diagnostics: HWND::default(),
            timer: HWND::default(),
            status: HWND::default(),
            others: Vec::new(),
            font: None,
            factor,
            current: current.clone(),
            change: None,
            start: None,
            return_to: foreground_key(),
            closed: false,
        };
        this.build(devices)?;
        this.fill_sounds(&current.prayer_sound);
        this.show(current);
        this.relayout()?;
        // SAFETY: explicit user request only. Windows retains its normal foreground restrictions.
        unsafe {
            SetWindowTextW(window, w!("ROQA settings"))?;
            let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            if SetForegroundWindow(window).as_bool() {
                SetFocus(Some(this.dock))?;
            }
        }
        Ok(this)
    }

    pub fn set_pins(&self, names: &[String], selected: usize) {
        // SAFETY: synchronous string copies to our own native combo box.
        unsafe {
            SendMessageW(self.pins, CB_RESETCONTENT, None, None);
            for name in names {
                let name = wide(name);
                SendMessageW(
                    self.pins,
                    CB_ADDSTRING,
                    None,
                    Some(LPARAM(name.as_ptr() as isize)),
                );
            }
            SendMessageW(
                self.pins,
                CB_SETCURSEL,
                Some(WPARAM(clamp_selection(selected as isize, names.len()))),
                None,
            );
        }
    }
    /// The pin combo's current row (0 when none), so a refresh keeps the user's place.
    pub fn pin_selection(&self) -> usize {
        // SAFETY: synchronous query of our own combo box.
        let row = unsafe { SendMessageW(self.pins, CB_GETCURSEL, None, None) }.0;
        clamp_selection(row, usize::MAX)
    }
    /// Selects pin row `row` (kept inside the list), so the next refresh keeps it.
    pub fn select_pin(&self, row: usize) {
        // SAFETY: synchronous queries and selection on our own combo box.
        unsafe {
            let count = SendMessageW(self.pins, CB_GETCOUNT, None, None).0;
            let row = clamp_selection(row as isize, count.max(0) as usize);
            SendMessageW(self.pins, CB_SETCURSEL, Some(WPARAM(row)), None);
        }
    }
    pub fn take_move(&mut self) -> Option<(usize, bool)> {
        self.move_pin.take()
    }
    pub fn refresh_monitors(&mut self, devices: &[String]) -> Result<()> {
        if self
            .monitors
            .iter()
            .map(|(_, device)| device)
            .eq(devices.iter().take(MAX_OFF))
        {
            return Ok(());
        }
        // Keep keyboard focus in a live control when the focused monitor disappears.
        // SAFETY: own controls and current thread focus; no foreground activation.
        unsafe {
            if self.monitors.iter().any(|(handle, _)| {
                *handle == windows::Win32::UI::Input::KeyboardAndMouse::GetFocus()
            }) {
                SetFocus(Some(self.dock))?;
            }
        }
        // Retain edit contents and settings; replace only monitor checkboxes.
        for (handle, _) in self.monitors.drain(..) {
            // SAFETY: owned monitor checkbox, no retained references.
            unsafe {
                DestroyWindow(handle)?;
            }
        }
        // Tab order follows the screen: each new checkbox goes right after the previous one.
        let mut previous = self.notch;
        for (index, device) in devices.iter().take(MAX_OFF).enumerate() {
            let handle = control(
                self.window,
                w!("Button"),
                &format!(
                    "Show on monitor {} ({})",
                    index + 1,
                    device.trim_start_matches(r"\\.\")
                ),
                MONITOR + index,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP.0,
                WINDOW_EX_STYLE(0),
            )?;
            // SAFETY: both are our own live children; this only reorders them, no move, size or activation.
            unsafe {
                SetWindowPos(
                    handle,
                    Some(previous),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )?;
            }
            previous = handle;
            set_checked(handle, !self.current.off_monitors.contains(device));
            self.monitors.push((handle, device.clone()));
        }
        self.relayout()
    }
    /// Creates the controls in tab order, which matches the top-to-bottom layout in `position`.
    fn build(&mut self, devices: &[String]) -> Result<()> {
        let window = self.window;
        let tab = WS_TABSTOP.0;
        let flat = WINDOW_EX_STYLE(0);
        let check = BS_AUTOCHECKBOX as u32 | tab;
        let edit = ES_AUTOHSCROLL as u32 | WS_BORDER.0 | tab;
        let label =
            |text: &str| control(window, w!("Static"), text, LABEL, 0, WS_EX_NOPARENTNOTIFY);
        self.dock = control(window, w!("Button"), "Show the &dock", DOCK, check, flat)?;
        self.notch = control(window, w!("Button"), "Show the &notch", NOTCH, check, flat)?;
        for (index, device) in devices.iter().take(MAX_OFF).enumerate() {
            let name = format!(
                "Show on monitor {} ({})",
                index + 1,
                device.trim_start_matches(r"\\.\")
            );
            let id = MONITOR + index;
            let handle = control(window, w!("Button"), &name, id, check, flat)?;
            self.monitors.push((handle, device.clone()));
        }
        self.sound = control(
            window,
            w!("Button"),
            "Play a s&ound when a timer finishes",
            SOUND,
            check,
            flat,
        )?;
        self.desktop = control(
            window,
            w!("Button"),
            "Show only windows on this &desktop",
            DESKTOP,
            check,
            flat,
        )?;
        self.edge_volume = control(
            window,
            w!("Button"),
            "&Volume slider on the right screen edge",
            EDGE_VOLUME,
            check,
            flat,
        )?;
        self.edge_brightness = control(
            window,
            w!("Button"),
            "&Brightness slider on the left screen edge",
            EDGE_BRIGHTNESS,
            check,
            flat,
        )?;
        // Tab order follows the screen: the tabs, left to right and top to bottom, then the mascot.
        for tab in Tab::ALL {
            let name = match tab {
                Tab::Media => "&Media tab",
                Tab::Weather => "&Weather tab",
                Tab::Prayer => "Prayer tab",
                Tab::Timer => "Timer ta&b",
                Tab::Shelf => "Shelf ta&b",
            };
            self.tabs[tab.index()] =
                control(window, w!("Button"), name, TAB + tab.index(), check, flat)?;
        }
        self.eyes = control(window, w!("Button"), "Mascot e&yes", EYES, check, flat)?;
        let theme_label = label("T&heme")?;
        self.theme = control(
            window,
            w!("ComboBox"),
            "",
            THEME,
            CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0 | tab,
            flat,
        )?;
        for theme in Theme::ALL {
            let name = wide(theme.label());
            // SAFETY: the combo box copies the terminated string synchronously.
            unsafe {
                SendMessageW(
                    self.theme,
                    CB_ADDSTRING,
                    None,
                    Some(LPARAM(name.as_ptr() as isize)),
                );
            }
        }
        let dock_label = label("Dock st&yle")?;
        self.dock_style = control(
            window,
            w!("ComboBox"),
            "",
            DOCKSTYLE,
            CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0 | tab,
            flat,
        )?;
        for style in DockStyle::ALL {
            let name = wide(style.label());
            // SAFETY: the combo box copies the terminated string synchronously.
            unsafe {
                SendMessageW(
                    self.dock_style,
                    CB_ADDSTRING,
                    None,
                    Some(LPARAM(name.as_ptr() as isize)),
                );
            }
        }
        let shelf_label = label(&format!("Shel&f size (1 to {})", shelf::MAX))?;
        self.shelf = control(
            window,
            w!("Edit"),
            "",
            SHELF,
            edit | ES_NUMBER as u32,
            WS_EX_CLIENTEDGE,
        )?;
        let city_label = label("Weather c&ity (empty is none)")?;
        self.city = control(window, w!("Edit"), "", CITY, edit, WS_EX_CLIENTEDGE)?;
        self.detect = control(
            window,
            w!("Button"),
            "Detect location a&utomatically",
            DETECT,
            check,
            flat,
        )?;
        self.units = control(
            window,
            w!("ComboBox"),
            "",
            UNITS,
            CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0 | tab,
            flat,
        )?;
        for units in Units::ALL {
            let name = wide(units.label());
            // SAFETY: the combo box copies the terminated string synchronously.
            unsafe {
                SendMessageW(
                    self.units,
                    CB_ADDSTRING,
                    None,
                    Some(LPARAM(name.as_ptr() as isize)),
                );
            }
        }
        self.prayer = control(
            window,
            w!("Button"),
            "Prayer &reminders",
            PRAYER,
            check,
            flat,
        )?;
        let combo = CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0 | tab;
        self.pmethod = control(
            window,
            w!("ComboBox"),
            "Prayer method",
            PMETHOD,
            combo,
            flat,
        )?;
        for method in Method::ALL {
            add_row(self.pmethod, method.label());
        }
        self.psound = control(window, w!("ComboBox"), "Prayer sound", PSOUND, combo, flat)?;
        self.pbrowse = control(window, w!("Button"), "Bro&wse...", PBROWSE, tab, flat)?;
        self.ptest = control(window, w!("Button"), "Te&st sound", PTEST, tab, flat)?;
        let lead_label = label("Heads-up before the prayer")?;
        self.plead = control(
            window,
            w!("ComboBox"),
            "Heads-up before the prayer",
            PLEAD,
            combo,
            flat,
        )?;
        for lead in LEADS {
            add_row(
                self.plead,
                &if lead == 0 {
                    "Off".to_owned()
                } else {
                    format!("{lead} minutes before")
                },
            );
        }
        let diagnostics_label = label("Dia&gnostics report file (empty is off)")?;
        self.diagnostics = control(window, w!("Edit"), "", DIAGNOSTICS, edit, WS_EX_CLIENTEDGE)?;
        let timer_label = label("&Timer length (minutes, m:ss or h:mm:ss)")?;
        self.timer = control(
            window,
            w!("Edit"),
            FIRST_TIMER,
            TIMER,
            edit,
            WS_EX_CLIENTEDGE,
        )?;
        let start = control(window, w!("Button"), "St&art timer", START, tab, flat)?;
        self.pin_label = control(
            window,
            w!("Static"),
            "Pinned apps",
            LABEL,
            0,
            WS_EX_NOPARENTNOTIFY,
        )?;
        self.pins = control(
            window,
            w!("ComboBox"),
            "Pinned apps",
            PIN,
            CBS_DROPDOWNLIST as u32 | tab | WS_VSCROLL.0,
            flat,
        )?;
        self.earlier = control(
            window,
            w!("Button"),
            "Move pin &earlier",
            EARLIER,
            tab,
            flat,
        )?;
        self.later = control(window, w!("Button"), "Move pin &later", LATER, tab, flat)?;
        let reset = control(window, w!("Button"), "&Reset to defaults", RESET, tab, flat)?;
        let close = control(window, w!("Button"), "&Close", CLOSE, tab, flat)?;
        self.status = control(window, w!("Static"), "", STATUS, 0, WS_EX_NOPARENTNOTIFY)?;
        self.others = vec![
            theme_label,
            shelf_label,
            city_label,
            diagnostics_label,
            timer_label,
            start,
            reset,
            close,
            lead_label,
            dock_label,
        ];
        // SAFETY: scalar limits on our own edits; Windows bounds what a user can type or paste.
        unsafe {
            for (edit, limit) in [
                (self.shelf, 3),
                (self.city, MAX_CITY),
                (self.diagnostics, MAX_PATH),
                (self.timer, 8),
            ] {
                SendMessageW(edit, EM_SETLIMITTEXT, Some(WPARAM(limit)), Some(LPARAM(0)));
            }
        }
        Ok(())
    }

    fn children(&self) -> Vec<HWND> {
        let mut all = vec![
            self.dock,
            self.notch,
            self.sound,
            self.desktop,
            self.edge_volume,
            self.edge_brightness,
            self.theme,
            self.dock_style,
            self.shelf,
            self.city,
            self.detect,
            self.units,
            self.prayer,
            self.pmethod,
            self.psound,
            self.pbrowse,
            self.ptest,
            self.plead,
        ];
        all.extend(self.tabs);
        all.push(self.eyes);
        all.extend([self.pin_label, self.pins, self.earlier, self.later]);
        all.extend(self.monitors.iter().map(|(handle, _)| *handle));
        all.extend([self.diagnostics, self.timer, self.status]);
        all.extend(&self.others);
        all
    }

    /// Writes `settings` into the controls (not the timer length, which is not a setting).
    fn show(&self, settings: &Settings) {
        set_checked(self.dock, settings.dock);
        set_checked(self.notch, settings.notch);
        set_checked(self.sound, settings.timer_sound);
        set_checked(self.desktop, settings.desktop_only);
        set_checked(self.edge_volume, settings.edge_volume);
        set_checked(self.edge_brightness, settings.edge_brightness);
        for tab in Tab::ALL {
            set_checked(self.tabs[tab.index()], settings.tabs[tab.index()]);
        }
        set_checked(self.eyes, settings.eyes);
        let index = Theme::ALL
            .iter()
            .position(|t| *t == settings.theme)
            .unwrap_or(0);
        // SAFETY: scalar selection index on our own combo box.
        unsafe {
            SendMessageW(self.theme, CB_SETCURSEL, Some(WPARAM(index)), None);
        }
        let style = DockStyle::ALL
            .iter()
            .position(|s| *s == settings.dock_style)
            .unwrap_or(0);
        // SAFETY: scalar selection index on our own combo box.
        unsafe {
            SendMessageW(self.dock_style, CB_SETCURSEL, Some(WPARAM(style)), None);
        }
        for (handle, device) in &self.monitors {
            set_checked(*handle, !settings.off_monitors.contains(device));
        }
        let _ = set_text(self.shelf, &settings.shelf_cap.to_string());
        let _ = set_text(self.city, &settings.city);
        set_checked(self.detect, settings.detect);
        let units = Units::ALL
            .iter()
            .position(|u| *u == settings.units)
            .unwrap_or(0);
        // SAFETY: scalar selection index on our own combo box.
        unsafe {
            SendMessageW(self.units, CB_SETCURSEL, Some(WPARAM(units)), None);
        }
        let _ = set_text(
            self.diagnostics,
            settings.diagnostics.as_deref().unwrap_or(""),
        );
        set_checked(self.prayer, settings.prayer_remind);
        select(
            self.pmethod,
            Method::ALL
                .iter()
                .position(|m| *m == settings.prayer_method),
        );
        select(
            self.plead,
            LEADS.iter().position(|l| *l == settings.prayer_lead),
        );
        select(self.psound, Some(self.sound_row(&settings.prayer_sound)));
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
        // SAFETY: our own combo box.
        unsafe {
            SendMessageW(self.psound, CB_RESETCONTENT, None, None);
        }
        add_row(self.psound, "None (silent)");
        for sound in &self.sounds {
            add_row(self.psound, prayer_tab::label(sound));
        }
    }

    /// The host stored a file the user browsed to: list it and select it.
    pub fn set_sound(&mut self, setting: &str) {
        self.current.prayer_sound = setting.to_owned();
        self.fill_sounds(setting);
        select(self.psound, Some(self.sound_row(setting)));
    }

    /// What the user asked of the sound since the host last asked.
    pub fn take_sound_action(&mut self) -> Option<SoundAction> {
        self.sound_action.take()
    }

    /// Reads the controls, brings them into range, shows what was kept and hands a real change to the host.
    fn commit(&mut self) -> Result<()> {
        let devices: Vec<String> = self.monitors.iter().map(|(_, d)| d.clone()).collect();
        let mut next = self.current.clone();
        next.dock = checked(self.dock);
        next.notch = checked(self.notch);
        next.timer_sound = checked(self.sound);
        next.desktop_only = checked(self.desktop);
        next.edge_volume = checked(self.edge_volume);
        next.edge_brightness = checked(self.edge_brightness);
        for tab in Tab::ALL {
            next.tabs[tab.index()] = checked(self.tabs[tab.index()]);
        }
        next.eyes = checked(self.eyes);
        // SAFETY: scalar query of our own combo box; -1 (nothing chosen) keeps the current theme.
        let chosen = unsafe { SendMessageW(self.theme, CB_GETCURSEL, None, None) }.0;
        next.theme = usize::try_from(chosen)
            .ok()
            .and_then(|index| Theme::ALL.get(index).copied())
            .unwrap_or(next.theme);
        // A monitor that is not connected now keeps its stored state.
        next.off_monitors.retain(|d| !devices.contains(d));
        next.off_monitors.extend(
            self.monitors
                .iter()
                .filter(|(handle, _)| !checked(*handle))
                .map(|(_, d)| d.clone()),
        );
        next.shelf_cap = text(self.shelf, 3).trim().parse().unwrap_or(next.shelf_cap);
        next.city = text(self.city, MAX_CITY);
        next.detect = checked(self.detect);
        // SAFETY: scalar query of our own combo box; -1 (nothing chosen) keeps the current units.
        let chosen = unsafe { SendMessageW(self.units, CB_GETCURSEL, None, None) }.0;
        next.units = usize::try_from(chosen)
            .ok()
            .and_then(|index| Units::ALL.get(index).copied())
            .unwrap_or(next.units);
        next.dock_style = chosen_row(self.dock_style)
            .and_then(|i| DockStyle::ALL.get(i).copied())
            .unwrap_or(next.dock_style);
        next.diagnostics = Some(text(self.diagnostics, MAX_PATH));
        next.prayer_remind = checked(self.prayer);
        next.prayer_method = chosen_row(self.pmethod)
            .and_then(|i| Method::ALL.get(i).copied())
            .unwrap_or(next.prayer_method);
        next.prayer_lead = chosen_row(self.plead)
            .and_then(|i| LEADS.get(i).copied())
            .unwrap_or(next.prayer_lead);
        // A row differing from what the stored setting shows is a pick; otherwise "default" stays "default".
        if let Some(row) =
            chosen_row(self.psound).filter(|&r| r != self.sound_row(&next.prayer_sound))
        {
            next.prayer_sound = match row {
                0 => SILENT.to_owned(),
                n => self.sounds.get(n - 1).cloned().unwrap_or_default(),
            };
        }
        let kept_a_kind = next.dock || next.notch;
        let ranged = next.clamped();
        let next = ranged.clone().keep_one_monitor(&devices);
        self.show(&next);
        if !kept_a_kind {
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
                let monitor = (MONITOR..MONITOR + self.monitors.len()).contains(&id);
                match id {
                    PBROWSE | PTEST if notification == BN_CLICKED => {
                        self.sound_action = Some(if id == PTEST {
                            SoundAction::Test
                        } else {
                            SoundAction::Browse
                        });
                    }
                    PMETHOD | PSOUND | PLEAD if notification == CBN_SELCHANGE => self.commit()?,
                    DOCK | NOTCH | SOUND | DESKTOP | EDGE_VOLUME | EDGE_BRIGHTNESS | DETECT
                    | PRAYER
                        if notification == BN_CLICKED =>
                    {
                        self.commit()?
                    }
                    EARLIER | LATER if notification == BN_CLICKED => {
                        // SAFETY: scalar selection query of our own native combo.
                        let index = unsafe { SendMessageW(self.pins, CB_GETCURSEL, None, None) }.0;
                        if index >= 0 {
                            self.move_pin = Some((index as usize, id == LATER));
                        }
                    }
                    TAB..=EYES if notification == BN_CLICKED => self.commit()?,
                    THEME | UNITS | DOCKSTYLE if notification == CBN_SELCHANGE => self.commit()?,
                    _ if monitor && notification == BN_CLICKED => self.commit()?,
                    SHELF | CITY | DIAGNOSTICS if notification == EN_KILLFOCUS => self.commit()?,
                    OK => self.commit()?,
                    START => match parse_timer(&text(self.timer, 8)) {
                        Some(seconds) => self.start = Some(seconds),
                        None => {
                            self.say("Enter minutes, m:ss or h:mm:ss, from 1 second to 24 hours.")?
                        }
                    },
                    RESET => {
                        self.show(&Settings::default());
                        self.commit()?;
                    }
                    CLOSE => {
                        self.commit()?; // an edit still being typed is kept
                        self.closed = true;
                        restore_foreground(self.window, self.return_to);
                    }
                    _ => {}
                }
            }
            RELAYOUT => self.relayout()?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    pub fn keyboard(&self, message: &MSG) -> bool {
        // SAFETY: no Rust state is exposed to the reentrant dialog procedure; it posts scalar actions only.
        unsafe { IsDialogMessageW(self.window, message).as_bool() }
    }

    /// The Windows text size changed.
    pub fn set_factor(&mut self, factor: f64) -> Result<()> {
        self.factor = factor;
        self.relayout()
    }

    fn relayout(&mut self) -> Result<()> {
        let work = native_window::primary_work_area()?;
        // SAFETY: owned dialog is live.
        let dpi = unsafe { GetDpiForWindow(self.window) }.max(96);
        let font = PickerFont::create(dpi, self.factor)?;
        for child in self.children() {
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
        self.position(scale, dpi, work)
    }

    fn position(&self, scale: f64, dpi: u32, work: Bounds) -> Result<()> {
        let px = |value: f64| (value * scale).round() as i32;
        // Row heights in DIPs, top to bottom: one list gives both the window height and every control's top.
        // dock, notch, monitors, sound, desktop, two edge sliders, then the tabs and mascot in two columns
        let mut heights = vec![24.0; 6 + self.monitors.len() + 3];
        // theme, dock style, shelf, weather city, detect and units, diagnostics label and edit, timer label and row,
        // status, pins, buttons
        heights.extend([
            28.0, 28.0, 28.0, 28.0, 28.0, 28.0, 28.0, 28.0, 20.0, 28.0, 20.0, 28.0, 42.0, 28.0,
            28.0,
        ]);
        let tops: Vec<i32> = heights
            .iter()
            .scan(16.0, |y, height| {
                let top = *y;
                *y += height + 8.0;
                Some(px(top))
            })
            .collect();
        let last = heights.len() - 1;
        let mut frame = RECT {
            right: px(480.0),
            bottom: tops[last] + px(heights[last] + 16.0),
            ..Default::default()
        };
        // SAFETY: fills the local RECT for the dialog's fixed style at this DPI; positions only our own windows.
        unsafe {
            AdjustWindowRectExForDpi(
                &mut frame,
                WINDOW_STYLE(DIALOG_STYLE),
                false,
                WINDOW_EX_STYLE(0),
                dpi,
            )?;
            let width = (frame.right - frame.left).min(work.width);
            let height = (frame.bottom - frame.top).min(work.height);
            SetWindowPos(
                self.window,
                None,
                work.x + (work.width - width) / 2,
                work.y + (work.height - height) / 2,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )?;
            let mut client = RECT::default();
            GetClientRect(self.window, &mut client)?;
            let margin = px(16.0);
            let inner = (client.right - margin * 2).max(1);
            let half = |units: f64| px(units).min(inner);
            let after = 3 + self.monitors.len(); // the desktop checkbox, then two more
            let grid = after + 3; // three rows of two checkboxes: the five tabs, then the mascot
            let end = grid + 2;
            let mut places = vec![
                (self.dock, margin, tops[0], inner, px(24.0)),
                (self.notch, margin, tops[1], inner, px(24.0)),
                (self.sound, margin, tops[after - 1], inner, px(24.0)),
                (self.desktop, margin, tops[after], inner, px(24.0)),
                (self.edge_volume, margin, tops[after + 1], inner, px(24.0)),
                (
                    self.edge_brightness,
                    margin,
                    tops[after + 2],
                    inner,
                    px(24.0),
                ),
            ];
            for (index, (handle, _)) in self.monitors.iter().enumerate() {
                places.push((*handle, margin, tops[2 + index], inner, px(24.0)));
            }
            let column = (inner / 2).max(1);
            for (index, handle) in self.tabs.iter().chain([&self.eyes]).enumerate() {
                places.push((
                    *handle,
                    margin + column * (index as i32 % 2),
                    tops[grid + index / 2],
                    column,
                    px(24.0),
                ));
            }
            let [
                theme_label,
                shelf_label,
                city_label,
                diagnostics_label,
                timer_label,
                start,
                reset,
                close,
                lead_label,
                dock_label,
            ] = self.others[..]
            else {
                return Err(Error::new(E_FAIL, "Settings controls missing."));
            };
            places.extend([
                (
                    self.pin_label,
                    margin,
                    tops[last - 1] + px(5.0),
                    px(80.0),
                    px(20.0),
                ),
                (
                    self.pins,
                    margin + px(80.0),
                    tops[last - 1],
                    (inner - px(320.0)).max(1),
                    px(180.0),
                ),
                (
                    self.earlier,
                    margin + (inner - px(240.0)).max(1),
                    tops[last - 1],
                    px(120.0),
                    px(28.0),
                ),
                (
                    self.later,
                    margin + (inner - px(120.0)).max(1),
                    tops[last - 1],
                    px(120.0),
                    px(28.0),
                ),
                (
                    theme_label,
                    margin,
                    tops[end + 1] + px(5.0),
                    inner - half(180.0),
                    px(20.0),
                ),
                // The height is the dropped-down list's; the closed box is one row.
                (
                    self.theme,
                    margin + inner - half(170.0),
                    tops[end + 1],
                    half(170.0),
                    px(160.0),
                ),
                (
                    dock_label,
                    margin,
                    tops[end + 2] + px(5.0),
                    inner - half(260.0),
                    px(20.0),
                ),
                (
                    self.dock_style,
                    margin + inner - half(250.0),
                    tops[end + 2],
                    half(250.0),
                    px(120.0),
                ),
                (
                    shelf_label,
                    margin,
                    tops[end + 3] + px(5.0),
                    inner - half(80.0),
                    px(20.0),
                ),
                (
                    self.shelf,
                    margin + inner - half(70.0),
                    tops[end + 3],
                    half(70.0),
                    px(28.0),
                ),
                (
                    city_label,
                    margin,
                    tops[end + 4] + px(5.0),
                    inner - half(230.0),
                    px(20.0),
                ),
                (
                    self.city,
                    margin + inner - half(220.0),
                    tops[end + 4],
                    half(220.0),
                    px(28.0),
                ),
                (
                    self.detect,
                    margin,
                    tops[end + 5] + px(2.0),
                    inner - half(190.0),
                    px(24.0),
                ),
                (
                    self.units,
                    margin + inner - half(180.0),
                    tops[end + 5],
                    half(180.0),
                    px(160.0),
                ),
                (
                    self.prayer,
                    margin,
                    tops[end + 6] + px(2.0),
                    inner - half(240.0),
                    px(24.0),
                ),
                (
                    self.pmethod,
                    margin + inner - half(230.0),
                    tops[end + 6],
                    half(230.0),
                    px(160.0),
                ),
                (
                    self.psound,
                    margin,
                    tops[end + 7],
                    (inner - half(200.0)).max(1),
                    px(160.0),
                ),
                (
                    self.pbrowse,
                    margin + (inner - half(190.0)).max(0),
                    tops[end + 7],
                    half(90.0),
                    px(28.0),
                ),
                (
                    self.ptest,
                    margin + (inner - half(94.0)).max(0),
                    tops[end + 7],
                    half(94.0),
                    px(28.0),
                ),
                (
                    lead_label,
                    margin,
                    tops[end + 8] + px(5.0),
                    inner - half(190.0),
                    px(20.0),
                ),
                (
                    self.plead,
                    margin + inner - half(180.0),
                    tops[end + 8],
                    half(180.0),
                    px(160.0),
                ),
                (diagnostics_label, margin, tops[end + 9], inner, px(20.0)),
                (self.diagnostics, margin, tops[end + 10], inner, px(28.0)),
                (timer_label, margin, tops[end + 11], inner, px(20.0)),
                (self.timer, margin, tops[end + 12], half(120.0), px(28.0)),
                (
                    start,
                    margin + half(128.0),
                    tops[end + 12],
                    half(150.0),
                    px(28.0),
                ),
                (self.status, margin, tops[end + 13], inner, px(42.0)),
                (reset, margin, tops[last], half(180.0), px(28.0)),
                (
                    close,
                    client.right - margin - half(100.0),
                    tops[last],
                    half(100.0),
                    px(28.0),
                ),
            ]);
            for (child, x, y, w, h) in places {
                SetWindowPos(child, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE)?;
            }
        }
        Ok(())
    }
}

impl Drop for SettingsWindow {
    fn drop(&mut self) {
        // SAFETY: sole owner; destroying the parent synchronously destroys all child controls before the font drops.
        unsafe {
            if let Err(error) = DestroyWindow(self.window) {
                eprintln!("Settings window cleanup: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::clamp_selection;

    #[test]
    fn pin_row_stays_inside_a_refreshed_list() {
        assert_eq!(clamp_selection(2, 5), 2);
        // A pin was removed while the last row was selected: the row follows the shorter list.
        assert_eq!(clamp_selection(4, 3), 2);
        // CB_ERR (-1) and an empty list both give row 0.
        assert_eq!((clamp_selection(-1, 3), clamp_selection(3, 0)), (0, 0));
    }
}
