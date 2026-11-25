//! The Settings window's fixed frame and its one scrolling viewport.
//!
//! The rail, heading, local tabs, footer and scroll bar are fixed. The active page is one content window inside one
//! clipped viewport window; its controls are children of that content window, laid out once, and a scroll is a
//! single move of the content window (the system clips it to the viewport and repaints only the strip it uncovers).
//! The content window forwards its controls' messages to the dialog, which handles them as it always has.

use super::*;
use windows::Win32::{
    Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE},
    System::LibraryLoader::GetModuleHandleW,
};

/// Where each fixed part goes, in client pixels (empty rectangles are parts that are absent).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Frame {
    /// No rail: a category picker above the page instead.
    pub compact: bool,
    pub rail: RECT,
    pub picker: RECT,
    pub title: RECT,
    pub caption: RECT,
    pub search: RECT,
    pub tabs: RECT,
    pub viewport: RECT,
    /// The scroll bar's track: 14 DIP wide beside the viewport, inset 4 at both ends.
    pub gutter: RECT,
    pub foot: RECT,
    pub status: RECT,
    pub close: RECT,
}

impl Frame {
    /// The viewport and its bar: where a wheel turn scrolls the page.
    pub fn scrolls_at(&self, x: i32, y: i32) -> bool {
        (self.viewport.left..self.gutter.right).contains(&x)
            && (self.viewport.top..self.viewport.bottom).contains(&y)
    }
}

/// The frame of a `width` by `height` pixel client at `scale` (DPI times text size). The rail needs 720 by 584
/// DIPs; below that the category picker is the page title and the margins shrink.
pub(super) fn frame(width: i32, height: i32, scale: f64, tabs: bool) -> Frame {
    let dip = |px: i32| (f64::from(px) / scale).round() as i32;
    let px = |v: i32| (f64::from(v) * scale).round() as i32;
    let (dw, dh) = (dip(width), dip(height));
    let compact = dw < 720 || dh < 584;
    let short = dh < 400;
    let margin = match (compact, dw < 400) {
        (false, _) => 24,
        (true, false) => 16,
        (true, true) => 12,
    };
    let left = if compact { margin } else { 184 + margin };
    let right = (dw - margin - 14).max(left + 1);
    let stacked = right - left < 360;
    let foot_top = (dh - if stacked { 82 } else { 52 }).max(1);
    // The band under the heading: the caption, then the local tabs, then the viewport.
    let (mut band, gap) = if compact {
        (if short { 52 } else { 86 }, 6)
    } else {
        (84, 12)
    };
    let search_stacked = compact || right - left < 490;
    let search_left = if search_stacked { left } else { right - 232 };
    let search_top = if search_stacked { band } else { 24 };
    if search_stacked {
        band += 40;
    }
    let top = if tabs { band + 32 + gap } else { band }
        .min(foot_top - 1)
        .max(0);
    let rect = |l: i32, t: i32, r: i32, b: i32| RECT {
        left: px(l),
        top: px(t),
        right: px(r),
        bottom: px(b),
    };
    let none = RECT::default();
    Frame {
        compact,
        rail: if compact { none } else { rect(0, 62, 184, dh) },
        picker: if compact {
            rect(margin, 8, dw - margin, 44)
        } else {
            none
        },
        title: if compact {
            none
        } else {
            rect(
                left,
                22,
                if search_stacked {
                    right
                } else {
                    search_left - 16
                },
                58,
            )
        },
        caption: if short && compact {
            none
        } else {
            rect(left, 60, right, 78)
        },
        search: rect(search_left, search_top, right, search_top + 32),
        tabs: if tabs {
            rect(left, band, right, band + 32)
        } else {
            none
        },
        viewport: rect(left, top, right, foot_top),
        gutter: rect(right, top + 4, right + 14, (foot_top - 4).max(top + 4)),
        foot: rect(if compact { 0 } else { 184 }, foot_top, dw, dh),
        status: if stacked {
            rect(left, foot_top + 8, right, dh - 46)
        } else {
            rect(left, foot_top + 8, right - 100, dh - 8)
        },
        close: rect((right - 84).max(left), dh - 42, right, dh - 10),
    }
}

/// What the viewport and the content windows are called.
const VIEW: PCWSTR = w!("RoqaSettingsView");
const PAGE: PCWSTR = w!("RoqaSettingsPage");

/// Registers the two window classes (a second open finds them already there).
fn register() -> Result<()> {
    // SAFETY: the module handle of this process.
    let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }?.0);
    let classes: [(PCWSTR, WNDPROC); 2] = [(VIEW, Some(view_proc)), (PAGE, Some(page_proc))];
    for (name, procedure) in classes {
        let class = WNDCLASSW {
            lpfnWndProc: procedure,
            hInstance: instance,
            lpszClassName: name,
            ..Default::default()
        };
        // SAFETY: a plain class with a static name and a procedure that lives for the process.
        let registered = unsafe { RegisterClassW(&class) } != 0;
        // SAFETY: a scalar read of this thread's last error.
        if !registered && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
            return Err(windows::core::Error::from_thread());
        }
    }
    Ok(())
}

fn make(parent: HWND, class: PCWSTR, style: u32) -> Result<HWND> {
    register()?;
    // SAFETY: the module handle of this process.
    let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }?.0);
    // SAFETY: a child of our own live window; the class was registered above.
    unsafe {
        CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class,
            None,
            WINDOW_STYLE(WS_CHILD.0 | WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0 | style),
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(instance),
            None,
        )
    }
}

/// The clipped window the page scrolls inside. It starts a Tab group so the local tabs end before it.
pub(super) fn viewport(dialog: HWND) -> Result<HWND> {
    make(dialog, VIEW, WS_VISIBLE.0 | WS_GROUP.0)
}

/// One page's content window (hidden until its page is shown): the parent of that page's controls.
pub(super) fn page(viewport: HWND) -> Result<HWND> {
    make(viewport, PAGE, 0)
}

/// The messages a control sends its parent that the dialog's procedure answers.
fn forwarded(message: u32) -> bool {
    matches!(
        message,
        WM_COMMAND
            | WM_NOTIFY
            | WM_DRAWITEM
            | WM_CTLCOLORSTATIC
            | WM_CTLCOLORBTN
            | WM_CTLCOLOREDIT
            | WM_CTLCOLORLISTBOX
    )
}

/// The dialog above the viewport above this content window.
fn dialog_of(h: HWND) -> HWND {
    // SAFETY: a scalar query of a live window.
    unsafe { GetAncestor(h, GA_ROOT) }
}

unsafe extern "system" fn view_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_ERASEBKGND {
        let content = PAINT.with(|cell| {
            cell.try_borrow()
                .ok()
                .and_then(|p| p.as_ref().map(|p| p.pal.content))
        });
        if let Some(colour) = content {
            let mut client = RECT::default();
            // SAFETY: our own live window and the DC this message carries.
            unsafe {
                let _ = GetClientRect(hwnd, &mut client);
                fill(HDC(wparam.0 as *mut _), client, colour);
            }
            return LRESULT(1);
        }
    }
    // SAFETY: the default procedure for everything else.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

unsafe extern "system" fn page_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_ERASEBKGND {
        let drawn = PAINT.with(|cell| {
            let guard = cell.try_borrow().ok()?;
            let p = guard.as_ref()?;
            paint::page(HDC(wparam.0 as *mut _), hwnd, p);
            Some(())
        });
        if drawn.is_some() {
            return LRESULT(1);
        }
    } else if message == WM_LBUTTONUP {
        let y = i32::from((lparam.0 >> 16) as i16);
        let hit = PAINT.with(|cell| {
            cell.try_borrow()
                .ok()?
                .as_ref()?
                .results
                .as_ref()?
                .click(hwnd, y)
        });
        if let Some((index, revision)) = hit {
            // SAFETY: our own dialog receives scalar result identity, never a pointer.
            unsafe {
                let _ = PostMessageW(
                    Some(dialog_of(hwnd)),
                    search::OPEN,
                    WPARAM(index),
                    LPARAM(revision),
                );
            }
            return LRESULT(0);
        }
    } else if forwarded(message) {
        // SAFETY: the dialog is our own live window; the answer is the dialog procedure's, unchanged.
        return unsafe { SendMessageW(dialog_of(hwnd), message, Some(wparam), Some(lparam)) };
    }
    // SAFETY: the default procedure for everything else.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_design_size_puts_every_part_where_the_spec_says() {
        let f = frame(880, 680, 1.0, false);
        assert!(!f.compact);
        let r = |l, t, r, b| RECT {
            left: l,
            top: t,
            right: r,
            bottom: b,
        };
        assert_eq!(f.viewport, r(208, 84, 842, 628));
        assert_eq!(f.gutter, r(842, 88, 856, 624));
        assert_eq!(f.rail, r(0, 62, 184, 680));
        assert_eq!(f.title, r(208, 22, 594, 58));
        assert_eq!(f.search, r(610, 24, 842, 56));
        assert_eq!(f.caption, r(208, 60, 842, 78));
        assert_eq!(f.foot, r(184, 628, 880, 680));
        assert_eq!(f.close, r(758, 638, 842, 670));
        assert_eq!(
            f.close.right, f.viewport.right,
            "Close trails the content edge"
        );
        assert!(
            f.status.right <= f.close.left,
            "the message stops short of Close"
        );
        // Local tabs push the viewport down 44 and take a 32 high band.
        let tabbed = frame(880, 680, 1.0, true);
        assert_eq!(tabbed.tabs, r(208, 84, 842, 116));
        assert_eq!(tabbed.viewport.top, 128);
    }

    #[test]
    fn the_frame_scales_with_the_screen_and_stays_inside_it() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for (w, h) in [(880, 680), (720, 584), (600, 472), (400, 300), (320, 240)] {
                let (cw, ch) = (
                    (f64::from(w) * scale).round() as i32,
                    (f64::from(h) * scale).round() as i32,
                );
                for tabs in [false, true] {
                    let f = frame(cw, ch, scale, tabs);
                    for part in [f.search, f.viewport, f.gutter, f.foot, f.status, f.close] {
                        assert!(
                            part.right > part.left && part.bottom >= part.top,
                            "{w}x{h} at {scale}: {part:?}"
                        );
                        assert!(
                            part.right <= cw + 1 && part.bottom <= ch + 1,
                            "{w}x{h} at {scale}: {part:?}"
                        );
                    }
                    assert!(
                        f.viewport.top < f.viewport.bottom,
                        "{w}x{h}: a viewport remains"
                    );
                    assert!(
                        f.viewport.bottom <= f.foot.top + 1,
                        "the page never runs under the footer"
                    );
                }
            }
        }
    }

    #[test]
    fn the_rail_needs_room_and_compact_replaces_it_with_a_picker() {
        assert!(!frame(720, 584, 1.0, false).compact);
        assert!(frame(719, 584, 1.0, false).compact);
        assert!(frame(720, 583, 1.0, false).compact);
        let c = frame(600, 472, 1.0, true);
        assert_eq!(c.rail, RECT::default());
        assert_eq!((c.picker.top, c.picker.bottom), (8, 44));
        assert_eq!(
            (c.caption.top, c.search.top, c.tabs.top, c.viewport.top),
            (60, 86, 126, 164)
        );
        assert_eq!(c.viewport.left, 16, "16 inline margin");
        let short = frame(600, 390, 1.0, false);
        assert_eq!(
            (short.caption, short.search.top, short.viewport.top),
            (RECT::default(), 52, 92)
        );
        assert_eq!(frame(600, 390, 1.0, true).viewport.top, 130);
        assert_eq!(frame(390, 500, 1.0, false).viewport.left, 12);
    }

    #[test]
    fn narrow_status_stacks_above_close_without_entering_the_viewport() {
        let frame = frame(320, 240, 1.0, true);
        assert_eq!(frame.status.right, frame.close.right);
        assert!(frame.status.bottom < frame.close.top);
        assert!(frame.viewport.bottom < frame.status.top);
    }

    #[test]
    fn only_the_viewport_and_its_bar_take_the_wheel() {
        let f = frame(880, 680, 1.0, false);
        assert!(f.scrolls_at(300, 300) && f.scrolls_at(848, 300));
        assert!(!f.scrolls_at(100, 300), "the rail does nothing");
        assert!(!f.scrolls_at(300, 40), "nor the heading");
        assert!(!f.scrolls_at(300, 650), "nor the footer");
    }
}
