//! The settings window (M5 S7): a modeless standard-control window in the `picker.rs` style, created on
//! demand and destroyed on close. Standard checkboxes, edits and buttons give tab order, mnemonics, Narrator
//! and text scaling by default. It owns no timer or thread: it exists only while open and does work only in
//! the message loop turns its own controls cause. A change is handed to the host (`take_change`) which applies
//! it live; a timer request likewise (`take_start`).

use crate::{
    dock::{foreground_key, restore_foreground},
    layout::Bounds,
    native_window,
    picker::{ACTION, CLOSE, DIALOG_STYLE, RELAYOUT, control, empty_dialog, wide},
    picker_font::PickerFont,
    settings::{MAX_OFF, MAX_PATH, Settings, parse_timer},
    shelf,
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
const MONITOR: usize = 120;
const OK: usize = 1;
/// Timer length shown when the window opens.
const FIRST_TIMER: &str = "5";

pub struct SettingsWindow {
    window: HWND,
    dock: HWND,
    notch: HWND,
    sound: HWND,
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
            dock: HWND::default(),
            notch: HWND::default(),
            sound: HWND::default(),
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

    /// Creates the controls in tab order.
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
        let shelf_label = label(&format!("S&helf size (1 to {})", shelf::MAX))?;
        self.shelf = control(
            window,
            w!("Edit"),
            "",
            SHELF,
            edit | ES_NUMBER as u32,
            WS_EX_CLIENTEDGE,
        )?;
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
        let reset = control(window, w!("Button"), "&Reset to defaults", RESET, tab, flat)?;
        let close = control(window, w!("Button"), "&Close", CLOSE, tab, flat)?;
        self.status = control(window, w!("Static"), "", STATUS, 0, WS_EX_NOPARENTNOTIFY)?;
        self.others = vec![
            shelf_label,
            diagnostics_label,
            timer_label,
            start,
            reset,
            close,
        ];
        // SAFETY: scalar limits on our own edits; Windows bounds what a user can type or paste.
        unsafe {
            for (edit, limit) in [
                (self.shelf, 3),
                (self.diagnostics, MAX_PATH),
                (self.timer, 8),
            ] {
                SendMessageW(edit, EM_SETLIMITTEXT, Some(WPARAM(limit)), Some(LPARAM(0)));
            }
        }
        Ok(())
    }

    fn children(&self) -> Vec<HWND> {
        let mut all = vec![self.dock, self.notch, self.sound, self.shelf];
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
        for (handle, device) in &self.monitors {
            set_checked(*handle, !settings.off_monitors.contains(device));
        }
        let _ = set_text(self.shelf, &settings.shelf_cap.to_string());
        let _ = set_text(
            self.diagnostics,
            settings.diagnostics.as_deref().unwrap_or(""),
        );
    }

    /// Reads the controls, brings them into range, shows what was kept and hands a real change to the host.
    fn commit(&mut self) -> Result<()> {
        let devices: Vec<String> = self.monitors.iter().map(|(_, d)| d.clone()).collect();
        let mut next = self.current.clone();
        next.dock = checked(self.dock);
        next.notch = checked(self.notch);
        next.timer_sound = checked(self.sound);
        // A monitor that is not connected now keeps its stored state.
        next.off_monitors.retain(|d| !devices.contains(d));
        next.off_monitors.extend(
            self.monitors
                .iter()
                .filter(|(handle, _)| !checked(*handle))
                .map(|(_, d)| d.clone()),
        );
        next.shelf_cap = text(self.shelf, 3).trim().parse().unwrap_or(next.shelf_cap);
        next.diagnostics = Some(text(self.diagnostics, MAX_PATH));
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
                    DOCK | NOTCH | SOUND if notification == BN_CLICKED => self.commit()?,
                    _ if monitor && notification == BN_CLICKED => self.commit()?,
                    SHELF | DIAGNOSTICS if notification == EN_KILLFOCUS => self.commit()?,
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
        let mut heights = vec![24.0; 3 + self.monitors.len()]; // dock, notch, monitors, sound
        heights.extend([28.0, 20.0, 28.0, 20.0, 28.0, 42.0, 28.0]);
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
            let after = 2 + self.monitors.len(); // the sound checkbox
            let mut places = vec![
                (self.dock, margin, tops[0], inner, px(24.0)),
                (self.notch, margin, tops[1], inner, px(24.0)),
                (self.sound, margin, tops[after], inner, px(24.0)),
            ];
            for (index, (handle, _)) in self.monitors.iter().enumerate() {
                places.push((*handle, margin, tops[2 + index], inner, px(24.0)));
            }
            let [
                shelf_label,
                diagnostics_label,
                timer_label,
                start,
                reset,
                close,
            ] = self.others[..]
            else {
                return Err(Error::new(E_FAIL, "Settings controls missing."));
            };
            places.extend([
                (
                    shelf_label,
                    margin,
                    tops[after + 1] + px(5.0),
                    inner - half(80.0),
                    px(20.0),
                ),
                (
                    self.shelf,
                    margin + inner - half(70.0),
                    tops[after + 1],
                    half(70.0),
                    px(28.0),
                ),
                (diagnostics_label, margin, tops[after + 2], inner, px(20.0)),
                (self.diagnostics, margin, tops[after + 3], inner, px(28.0)),
                (timer_label, margin, tops[after + 4], inner, px(20.0)),
                (self.timer, margin, tops[after + 5], half(120.0), px(28.0)),
                (
                    start,
                    margin + half(128.0),
                    tops[after + 5],
                    half(150.0),
                    px(28.0),
                ),
                (self.status, margin, tops[after + 6], inner, px(42.0)),
                (reset, margin, tops[after + 7], half(180.0), px(28.0)),
                (
                    close,
                    client.right - margin - half(100.0),
                    tops[after + 7],
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
