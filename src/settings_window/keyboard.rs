//! Keyboard use of the Settings window, and its scrolling: Tab and Shift+Tab move between controls (the dialog
//! manager, `IsDialogMessageW`), Space and Enter act on the focused one, Esc closes the window, and a control the
//! focus lands on below or above the view is scrolled in. The focused control wears a 2 px ink ring, shown only
//! after a key (Windows' focus cues), never after a click.

use super::*;
use windows::Win32::{
    Foundation::POINT,
    UI::Input::KeyboardAndMouse::{GetFocus, VK_RETURN, VK_SPACE, VK_TAB},
};

/// Whether `h` has the keyboard focus and Windows shows focus cues.
pub(super) fn focus_ring(h: HWND) -> bool {
    // SAFETY: scalar queries of a live child.
    unsafe {
        GetFocus() == h
            && SendMessageW(h, WM_QUERYUISTATE, None, None).0 as u32 & UISF_HIDEFOCUS == 0
    }
}

/// A capsule (or tile) of `fill`; when `focused` its outer 2 px are the ink ring.
pub(super) unsafe fn ringed(dc: HDC, r: RECT, radius: i32, fill: u32, focused: bool, p: &Paint) {
    let w = if focused {
        (2.0 * p.scale).round() as i32
    } else {
        0
    };
    // SAFETY: the caller's live DC.
    unsafe {
        if focused {
            round(dc, r, radius, p.pal.text, p.pal.text);
        }
        let inner = RECT {
            left: r.left + w,
            top: r.top + w,
            right: r.right - w,
            bottom: r.bottom - w,
        };
        round(dc, inner, radius - w, fill, fill);
    }
}

/// How far to scroll so a control spanning `top..bottom` (view coordinates) sits inside `0..view` with `margin`
/// to spare; 0 when it already does.
fn into_view(top: i32, bottom: i32, view: i32, margin: i32) -> i32 {
    if top < margin {
        top - margin
    } else if bottom > view - margin {
        (bottom - view + margin).min(top - margin)
    } else {
        0
    }
}

impl SettingsWindow {
    /// A key for the window or one of its controls; false leaves it to the host's usual dispatch.
    pub(super) fn key(&mut self, message: &MSG) -> Result<bool> {
        let h = message.hwnd;
        // SAFETY: a scalar query of our own window.
        let ours = h == self.window || unsafe { IsChild(self.window, h) }.as_bool();
        if !ours || !(WM_KEYFIRST..=WM_KEYLAST).contains(&message.message) {
            return Ok(false);
        }
        let key = (message.message == WM_KEYDOWN).then_some(message.wParam.0 as u16);
        let kind = self.items.iter().find(|i| i.h == h).map(|i| i.kind);
        // SAFETY: plain messages to our own live controls; the MSG lives through the call.
        unsafe {
            if key == Some(VK_RETURN.0)
                && (h == self.close || matches!(kind, Some(Kind::Check | Kind::Btn)))
            {
                SendMessageW(h, BM_CLICK, None, None);
                return Ok(true);
            }
            if matches!(key, Some(k) if k == VK_RETURN.0 || k == VK_SPACE.0)
                && matches!(kind, Some(Kind::Combo(_)))
                && SendMessageW(h, CB_GETDROPPEDSTATE, None, None).0 == 0
            {
                SendMessageW(h, CB_SHOWDROPDOWN, Some(WPARAM(1)), None);
                return Ok(true);
            }
            // Tab, Shift+Tab and arrows move the focus, Esc is Close (IDCANCEL), Enter in a field is OK; the rest
            // is translated and dispatched to the control as usual.
            if !IsDialogMessageW(self.window, message).as_bool() {
                return Ok(false);
            }
            if key == Some(VK_TAB.0) {
                // The first Tab turns focus cues on after the focus moved; the page list would not redraw for it.
                let _ = InvalidateRect(Some(GetFocus()), None, false);
            }
        }
        self.reveal_focus()?;
        Ok(true)
    }

    /// Scrolls the page so the focused row is in view.
    fn reveal_focus(&mut self) -> Result<()> {
        // SAFETY: a scalar focus query.
        let focus = unsafe { GetFocus() };
        let Some(scale) = self.font.as_ref().map(|f| f.scale) else {
            return Ok(());
        };
        if !self.items.iter().any(|i| i.h == focus) {
            return Ok(());
        }
        let px = |v: i32| (v as f64 * scale).round() as i32;
        let (mut r, mut client, mut origin) = (RECT::default(), RECT::default(), POINT::default());
        // SAFETY: rectangles of our own live window and child, into locals.
        unsafe {
            GetWindowRect(focus, &mut r)?;
            GetClientRect(self.window, &mut client)?;
            let _ = ClientToScreen(self.window, &mut origin);
        }
        let view = client.bottom - px(FOOT);
        let shift = into_view(r.top - origin.y, r.bottom - origin.y, view, px(12));
        self.scroll_to(self.scroll + shift)
    }

    fn scroll_to(&mut self, next: i32) -> Result<()> {
        let next = next.clamp(0, self.scroll_max);
        if next != self.scroll {
            self.scroll = next;
            self.layout()?;
        }
        Ok(())
    }

    pub(super) fn scroll_message(&mut self, action: usize) -> Result<()> {
        let mut scroll = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_ALL,
            ..Default::default()
        };
        // SAFETY: queries our own scroll bar; no callbacks into Rust state.
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
                _ => return Ok(()),
            }
        };
        self.scroll_to(next)
    }

    /// Every control is a Tab stop, in creation order: the sidebar, the rows top to bottom, Close.
    pub(super) fn tab_stops(&self) {
        let controls = self
            .items
            .iter()
            .map(|i| i.h)
            .chain([self.side, self.close]);
        for h in controls.filter(|h| !h.0.is_null()) {
            // SAFETY: style bits of our own live child.
            unsafe {
                let style = GetWindowLongPtrW(h, GWL_STYLE);
                SetWindowLongPtrW(h, GWL_STYLE, style | WS_TABSTOP.0 as isize);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::into_view;

    #[test]
    fn a_focused_row_scrolls_just_into_view() {
        assert_eq!(into_view(100, 140, 500, 12), 0, "already in view");
        assert_eq!(
            into_view(520, 560, 500, 12),
            72,
            "below: its bottom comes up"
        );
        assert_eq!(
            into_view(-30, 10, 500, 12),
            -42,
            "above: its top comes down"
        );
        assert_eq!(
            into_view(0, 900, 500, 12),
            -12,
            "taller than the view: its top wins"
        );
    }
}
