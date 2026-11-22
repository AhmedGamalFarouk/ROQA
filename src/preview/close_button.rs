//! An owned, nonactivating overlay keeps the close control above the DWM thumbnail.

use crate::{
    dock,
    graphics::{Glyph, draw_settings_glyph, round_gdi},
    layout::Bounds,
    native_window::NativeWindow,
    theme::{Palette, Rgb},
};
use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::Gdi::{
            BeginPaint, CreateRoundRectRgn, DeleteObject, EndPaint, InvalidateRect, PAINTSTRUCT,
            SetWindowRgn,
        },
        UI::{
            Controls::WM_MOUSELEAVE,
            Input::KeyboardAndMouse::{
                ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
            },
            WindowsAndMessaging::{
                MSG, WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT,
                WindowFromPoint,
            },
        },
    },
    core::{Error, Result},
};

/// `WM_MOUSEMOVE`'s key-state bit for the left button.
const MK_LBUTTON: usize = 0x0001;

pub(super) struct CloseButton {
    window: NativeWindow,
    palette: Palette,
    side: i32,
    hovered: bool,
    pressed: bool,
}

fn bounds(preview: Bounds, work: Bounds, side: i32) -> Bounds {
    // Tiny sources still get the full hit area; keep it inside the monitor's work area.
    Bounds {
        x: (preview.x + preview.width - side)
            .clamp(work.x, (work.x + work.width - side).max(work.x)),
        y: preview.y.min(preview.y + preview.height - side).max(work.y),
        width: side,
        height: side,
    }
}

fn color((r, g, b): Rgb) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

impl CloseButton {
    pub(super) fn create(owner: HWND, dpi: u32, palette: Palette) -> Result<Self> {
        let window = NativeWindow::create_preview("ROQA Preview Close", Some(owner))?;
        let side = dock::px(32, dpi);
        // SAFETY: our live HWND; successful SetWindowRgn transfers ownership of the region.
        unsafe {
            let region = CreateRoundRectRgn(0, 0, side + 1, side + 1, side, side);
            if region.0.is_null() {
                return Err(Error::from_thread());
            }
            if SetWindowRgn(window.0, Some(region), false) == 0 {
                let error = Error::from_thread();
                let _ = DeleteObject(region.into());
                return Err(error);
            }
        }
        Ok(Self {
            window,
            palette,
            side,
            hovered: false,
            pressed: false,
        })
    }

    pub(super) fn place(&self, preview: Bounds, work: Bounds) -> Result<()> {
        self.window.resize(bounds(preview, work, self.side))
    }

    pub(super) fn hwnd(&self) -> HWND {
        self.window.0
    }

    pub(super) fn show(&self) {
        self.window.show();
    }

    fn repaint(&self) {
        // SAFETY: invalidates our control only; Windows coalesces paints until the message pass.
        let _ = unsafe { InvalidateRect(Some(self.window.0), None, false) };
    }

    fn paint(&self) {
        let side = self.side;
        let rect = RECT {
            left: 0,
            top: 0,
            right: side,
            bottom: side,
        };
        let body = self.palette.body;
        let (fill, ink) = if self.hovered {
            (self.palette.ink, self.palette.on_ink)
        } else {
            ((body.0, body.1, body.2), self.palette.ink)
        };
        // SAFETY: this message owns a valid paint DC; EndPaint always validates the region.
        unsafe {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(self.window.0, &mut paint);
            round_gdi(dc, rect, side / 2, color(fill), color(fill));
            let inset = (side - side * 20 / 32) / 2;
            let glyph = RECT {
                left: inset,
                top: inset,
                right: side - inset,
                bottom: side - inset,
            };
            draw_settings_glyph(dc, glyph, Glyph::Close, color(ink));
            let _ = EndPaint(self.window.0, &paint);
        }
    }

    fn point(&mut self, message: &MSG) {
        // SAFETY: captured moves may be outside the circular region; query scalar HWND identity.
        let over = unsafe { WindowFromPoint(message.pt) == self.window.0 };
        if over != self.hovered {
            self.hovered = over;
            self.repaint();
        }
        if over {
            // SAFETY: requests one leave notification on our control; no idle timer.
            let _ = unsafe {
                TrackMouseEvent(&mut TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: self.window.0,
                    dwHoverTime: 0,
                })
            };
        }
    }

    /// Only a press and release on this control requests close; a cancelled drag does nothing.
    pub(super) fn message(&mut self, message: &MSG) -> bool {
        if message.hwnd != self.window.0 {
            return false;
        }
        match message.message {
            WM_PAINT => self.paint(),
            // The release was lost: drop the press and its capture, which would take every move over ROQA.
            WM_MOUSEMOVE if self.pressed && message.wParam.0 & MK_LBUTTON == 0 => {
                self.pressed = false;
                // SAFETY: releases this thread's pointer capture; harmless if it is already gone.
                let _ = unsafe { ReleaseCapture() };
            }
            WM_MOUSEMOVE => self.point(message),
            WM_MOUSELEAVE => {
                self.hovered = false;
                self.repaint();
            }
            WM_LBUTTONDOWN => {
                self.pressed = true;
                // SAFETY: capture belongs to our live control until release or cancellation.
                unsafe {
                    SetCapture(self.window.0);
                }
            }
            WM_LBUTTONUP => {
                let pressed = std::mem::take(&mut self.pressed);
                if pressed {
                    // SAFETY: releases this thread's pointer capture and reads scalar HWND identity.
                    unsafe {
                        let over = WindowFromPoint(message.pt) == self.window.0;
                        let _ = ReleaseCapture();
                        return over;
                    }
                }
            }
            WM_CAPTURECHANGED => self.pressed = false,
            _ => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_is_owned_without_taking_focus_and_cancelled_presses_stay_quiet() {
        use windows::Win32::UI::WindowsAndMessaging::{
            GW_OWNER, GetForegroundWindow, GetWindow, IsWindow,
        };
        let _class = crate::native_window::WindowClass::register().unwrap();
        let owner = NativeWindow::create_preview("Preview test", None).unwrap();
        // Neither test HWND is ever shown, moved onto a screen or given input.
        // SAFETY: reads scalar foreground identity without activating either hidden test window.
        let foreground = unsafe { GetForegroundWindow() };
        let mut close =
            CloseButton::create(owner.0, 144, crate::theme::Theme::Classic.palette()).unwrap();
        let hwnd = close.hwnd();
        // SAFETY: both owned HWNDs are live; this only queries their ownership.
        assert_eq!(unsafe { GetWindow(hwnd, GW_OWNER) }.unwrap(), owner.0);
        // SAFETY: a scalar identity query, with no input or activation.
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        assert_eq!(close.side, 48);
        let mut message = MSG {
            hwnd,
            message: WM_LBUTTONUP,
            ..Default::default()
        };
        assert!(!close.message(&message), "a stray release must not close");
        close.pressed = true;
        message.message = WM_CAPTURECHANGED;
        assert!(!close.message(&message));
        message.message = WM_LBUTTONUP;
        assert!(!close.message(&message), "a cancelled press must not close");
        assert!(!close.pressed);
        drop(close);
        // SAFETY: IsWindow accepts a stale handle and reads identity only.
        assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
    }

    #[test]
    fn close_stays_full_size_on_tiny_sources_and_negative_monitors() {
        let work = Bounds {
            x: -1920,
            y: -200,
            width: 1920,
            height: 1040,
        };
        let preview = Bounds {
            x: -1920,
            y: 100,
            width: 1,
            height: 1,
        };
        let button = bounds(preview, work, 48);
        assert_eq!(
            (button.x, button.y, button.width, button.height),
            (-1920, 53, 48, 48)
        );
        let at_top = bounds(Bounds { y: -200, ..preview }, work, 32);
        assert_eq!(at_top.y, -200);
    }

    #[test]
    fn close_aligns_with_the_preview_at_each_dpi() {
        let work = Bounds {
            x: 1920,
            y: 0,
            width: 1920,
            height: 1040,
        };
        let preview = Bounds {
            x: 2300,
            y: 600,
            width: 360,
            height: 200,
        };
        for dpi in [96, 120, 144, 192] {
            let side = dock::px(32, dpi);
            let button = bounds(preview, work, side);
            assert_eq!((button.x + button.width, button.y), (2660, 600));
            assert_eq!((button.width, button.height), (side, side));
        }
    }
}
