//! The reminder box: one line of plain words ("in 20 min stretch", "at 17:30 call mum") in a small modeless
//! dialog like the window picker, because the notch never takes keyboard focus. Enter or Set sets it.

use crate::{
    dialog::{ACTION, CLOSE, RELAYOUT, control, dialog_proc, empty_dialog, wide},
    layout::Bounds,
    picker_font::PickerFont,
    prayer_os, remind,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RECT, WPARAM},
        UI::{
            Controls::EM_SETLIMITTEXT,
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{SetFocus, VIRTUAL_KEY, VK_ESCAPE, VK_RETURN},
            WindowsAndMessaging::*,
        },
    },
    core::{PCWSTR, Result, w},
};

const LINE: usize = 100;
const STATUS: usize = 102;
const SET: usize = 1;

pub struct RemindBox {
    window: HWND,
    label: HWND,
    line: HWND,
    status: HWND,
    set: HWND,
    close: HWND,
    font: Option<PickerFont>,
    /// The work area of the monitor it centres on.
    work: Bounds,
    closed: bool,
    /// The reminder just set: seconds from now and its label; the host takes it once.
    made: Option<(u64, String)>,
}

impl RemindBox {
    pub fn create(work: Bounds) -> Result<Self> {
        let window = empty_dialog(300, 120, Some(dialog_proc))?;
        let text =
            |class, title, id, style, extended| control(window, class, title, id, style, extended);
        let none = WINDOW_EX_STYLE(0);
        let mut made = Self {
            window,
            label: text(w!("Static"), "Say when, then what", 99, 0, none)?,
            line: text(
                w!("Edit"),
                "",
                LINE,
                ES_AUTOHSCROLL as u32 | WS_BORDER.0,
                WS_EX_CLIENTEDGE,
            )?,
            status: text(
                w!("Static"),
                "In 20 min stretch. At 17:30 call mum.",
                STATUS,
                0,
                none,
            )?,
            set: text(w!("Button"), "Set", SET, BS_DEFPUSHBUTTON as u32, none)?,
            close: text(w!("Button"), "Cancel", CLOSE, 0, none)?,
            font: None,
            work,
            closed: false,
            made: None,
        };
        made.relayout()?;
        // SAFETY: an explicit user click; Windows keeps its normal foreground rules, so focus is only
        // moved when the foreground was granted.
        unsafe {
            SetWindowTextW(window, w!("ROQA reminder"))?;
            let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            if SetForegroundWindow(window).as_bool() {
                SetFocus(Some(made.line))?;
            }
        }
        Ok(made)
    }

    /// Handles this box's messages; true when `message` was its own.
    pub fn message(&mut self, message: &MSG) -> Result<bool> {
        if message.hwnd == self.line && message.message == WM_KEYDOWN {
            match VIRTUAL_KEY(message.wParam.0 as u16) {
                VK_RETURN => self.submit(),
                VK_ESCAPE => self.closed = true,
                _ => return Ok(false),
            }
            return Ok(true);
        }
        if message.hwnd != self.window {
            return Ok(false);
        }
        match message.message {
            ACTION => match message.wParam.0 & 0xffff {
                SET => self.submit(),
                CLOSE => self.closed = true,
                _ => {}
            },
            RELAYOUT => self.relayout()?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn submit(&mut self) {
        let mut text = [0u16; 129];
        // SAFETY: our own edit control; Windows writes at most the slice's length.
        let length = unsafe { GetWindowTextW(self.line, &mut text) }.max(0) as usize;
        let line = String::from_utf16_lossy(&text[..length]);
        match remind::parse(&line, prayer_os::local_now().secs as u32) {
            Some(made) => {
                self.made = Some(made);
                self.closed = true;
            }
            None => {
                let hint = wide("Try: in 20 min stretch");
                // SAFETY: our own static control; the terminated text is copied by the call.
                unsafe {
                    let _ = SetWindowTextW(self.status, PCWSTR(hint.as_ptr()));
                }
            }
        }
    }

    /// The reminder the box made, once.
    pub fn take(&mut self) -> Option<(u64, String)> {
        self.made.take()
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    fn relayout(&mut self) -> Result<()> {
        let work = self.work;
        // SAFETY: the owned dialog is live.
        let dpi = unsafe { GetDpiForWindow(self.window) }.max(96);
        let font = PickerFont::create(dpi, 1.0)?;
        let px = |value: f64| (value * font.scale).round() as i32;
        let (width, height) = (px(400.0).min(work.width), px(200.0).min(work.height));
        let children = [self.label, self.line, self.status, self.set, self.close];
        // SAFETY: only the owned dialog and its children are touched; resizing never activates.
        unsafe {
            for child in children {
                SendMessageW(
                    child,
                    WM_SETFONT,
                    Some(WPARAM(font.handle.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
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
            let (margin, line) = (px(16.0), px(30.0));
            let inner = (client.right - margin * 2).max(1);
            let buttons_y = client.bottom - margin - line;
            for (child, x, y, w, h) in [
                (self.label, margin, margin, inner, px(20.0)),
                (self.line, margin, margin + px(28.0), inner, line),
                (self.status, margin, margin + px(66.0), inner, px(40.0)),
                (self.set, margin, buttons_y, px(100.0), line),
                (
                    self.close,
                    client.right - margin - px(100.0),
                    buttons_y,
                    px(100.0),
                    line,
                ),
            ] {
                SetWindowPos(child, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE)?;
            }
            SendMessageW(
                self.line,
                EM_SETLIMITTEXT,
                Some(WPARAM(128)),
                Some(LPARAM(0)),
            );
        }
        self.font = Some(font);
        Ok(())
    }
}

impl Drop for RemindBox {
    fn drop(&mut self) {
        // SAFETY: sole owner; destroying the parent destroys the children before the font drops.
        unsafe {
            if let Err(error) = DestroyWindow(self.window) {
                eprintln!("Reminder box cleanup: {error}");
            }
        }
    }
}
