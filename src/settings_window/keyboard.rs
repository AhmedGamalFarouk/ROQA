//! Keyboard use of the Settings window, and its scrolling: Tab and Shift+Tab move between controls (the dialog
//! manager, `IsDialogMessageW`, which reaches the controls inside the viewport's nested windows), Space and Enter
//! act on the focused one, Esc closes the window, Page Up, Page Down and Ctrl+Home or End scroll the page when the
//! focused control has no use for them, and a control the focus lands on below or above the view is scrolled in. The focused control wears a 2 px ink ring, shown only
//! after a key (Windows' focus cues), never after a click.

use super::*;
use windows::Win32::{
    Foundation::POINT,
    UI::Input::KeyboardAndMouse::{
        GetFocus, GetKeyState, VK_CONTROL, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_NEXT, VK_PRIOR,
        VK_RETURN, VK_SPACE, VK_TAB, VK_UP,
    },
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
pub(super) fn ringed_in(
    b: &crate::graphics::GdiBatch<'_>,
    r: RECT,
    radius: i32,
    fill: u32,
    focused: bool,
    p: &Paint,
) {
    let w = if focused {
        (2.0 * p.scale).round() as i32
    } else {
        0
    };
    if focused {
        b.round(r, radius, swap(p.pal.text), swap(p.pal.text));
    }
    let inner = RECT {
        left: r.left + w,
        top: r.top + w,
        right: r.right - w,
        bottom: r.bottom - w,
    };
    let swapped = swap(fill);
    b.round(inner, radius - w, swapped, swapped);
}

/// A capsule (or tile) of `fill`; when `focused` its outer 2 px are the ink ring.
pub(super) unsafe fn ringed(dc: HDC, r: RECT, radius: i32, fill: u32, focused: bool, p: &Paint) {
    crate::graphics::batch_gdi(dc, r, |b| ringed_in(b, r, radius, fill, focused, p));
}

/// How far to scroll so a control spanning `top..bottom` (view coordinates) sits inside `0..view` with `margin`
/// to spare; 0 when it already does.
pub(super) fn into_view(top: i32, bottom: i32, view: i32, margin: i32) -> i32 {
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
        if h == self.search.edit && search::composing(h) {
            return Ok(false);
        }
        let kind = self.items.iter().find(|i| i.h == h).map(|i| i.kind);
        // SAFETY: plain messages to our own live controls; the MSG lives through the call.
        unsafe {
            if key == Some(u16::from(b'F'))
                && GetKeyState(i32::from(VK_CONTROL.0)) < 0
                && !self.any_combo_dropped()
            {
                let _ = SetFocus(Some(self.search.edit));
                SendMessageW(
                    self.search.edit,
                    EM_SETSEL,
                    Some(WPARAM(0)),
                    Some(LPARAM(-1)),
                );
                return Ok(true);
            }
            if key == Some(VK_ESCAPE.0) && self.search.is_open() && !self.any_combo_dropped() {
                self.leave_search(true)?;
                self.layout()?;
                let _ = SetFocus(Some(self.search.edit));
                return Ok(true);
            }
            if h == self.search.edit {
                match key {
                    Some(key) if key == VK_RETURN.0 => {
                        self.open_search_hit(None)?;
                        return Ok(true);
                    }
                    Some(key) if self.search.is_open() && (key == VK_DOWN.0 || key == VK_UP.0) => {
                        self.move_search(key == VK_DOWN.0)?;
                        return Ok(true);
                    }
                    _ => {}
                }
            }
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
            // The page's own keys, for a control that has no use for them (a drop-down, list or text caret does).
            let plain = matches!(kind, Some(Kind::Check | Kind::Btn));
            let paged = plain || matches!(kind, Some(Kind::Edit(_)));
            let ctrl = GetKeyState(i32::from(VK_CONTROL.0)) < 0;
            match key {
                Some(k) if paged && (k == VK_PRIOR.0 || k == VK_NEXT.0) => {
                    self.scroll_message(scroll::PAGE, if k == VK_PRIOR.0 { -1 } else { 1 })?;
                    return Ok(true);
                }
                Some(k) if ctrl && plain && (k == VK_HOME.0 || k == VK_END.0) => {
                    self.jump(if k == VK_HOME.0 { 0 } else { self.scroll_max })?;
                    return Ok(true);
                }
                _ => {}
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

    /// Scrolls the page so the focused row is in view (the whole row with a 6 DIP cushion; the control alone
    /// when the row is taller than the view). Only a Tab or Shift+Tab calls it: a wheel or a drag never moves
    /// the focus, and the focus never pulls the page back.
    pub(super) fn reveal_focus(&mut self) -> Result<()> {
        // SAFETY: a scalar focus query.
        let focus = unsafe { GetFocus() };
        self.reveal_control(focus)
    }

    pub(super) fn reveal_control(&mut self, focus: HWND) -> Result<()> {
        if self.search.is_open() {
            return Ok(());
        }
        let Some(scale) = self.font.as_ref().map(|f| f.scale) else {
            return Ok(());
        };
        let Some(item) = self
            .items
            .iter()
            .find(|i| i.h == focus && usize::from(i.page) == self.page)
        else {
            return Ok(());
        };
        let px = |v: i32| (v as f64 * scale).round() as i32;
        let view = self.frame.viewport.bottom - self.frame.viewport.top;
        let margin = px(6);
        let (mut top, mut bottom) = item.row.get();
        if bottom - top > view - 2 * margin {
            let (mut r, mut origin) = (RECT::default(), POINT::default());
            // SAFETY: rectangles of our own live window and child, into locals.
            unsafe {
                GetWindowRect(focus, &mut r)?;
                let _ = ClientToScreen(self.contents[self.page], &mut origin);
            }
            (top, bottom) = (r.top - origin.y, r.bottom - origin.y);
        }
        let shift = into_view(top - self.scroll, bottom - self.scroll, view, margin);
        self.jump(self.scroll + shift)
    }

    /// Every control is a Tab stop, in creation order: the rail (or the picker), the page's local tabs (one stop,
    /// the current), the page's controls top to bottom, Close. Hidden and disabled ones are skipped by the
    /// dialog manager, so only the shown page's controls are reached.
    pub(super) fn tab_stops(&self) {
        let controls = self.items.iter().map(|i| i.h).chain([
            self.side,
            self.nav,
            self.search.edit,
            self.close,
        ]);
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
        assert_eq!(
            into_view(-3, 40, 500, 6),
            -9,
            "a 6 DIP cushion above the whole row"
        );
    }
}
