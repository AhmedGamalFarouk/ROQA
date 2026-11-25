//! Scrolling the Settings page: the bar in the window's right gutter (drawn with the frame), its thumb drag and
//! track paging, the wheel, and the one move of the content window that every scroll is.
//!
//! A scroll changes one number (the offset) and moves one window; it never lays out, measures, reads settings or
//! writes them. Input is applied as it arrives: the wheel and the thumb go straight to the offset (a fine
//! touchpad delta moves its fraction, carried across messages), with no glide to queue behind.

use super::*;
use std::cell::Cell;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};

/// `SCROLL` message codes (the wheel's is `dialog::WHEEL`); the amount rides in `lParam`.
pub(super) const PAGE: usize = 1;
const DRAG: usize = 2;
/// A line is 18 DIPs; Windows' wheel setting says how many a notch scrolls (3 by default).
const LINE: i32 = 18;
/// The shortest thumb, and the visible thumb's width, in DIPs.
const MIN_THUMB: i32 = 28;
const THUMB_W: i32 = 6;

thread_local! {
    /// Where in the thumb it was grabbed (pixels from its top) while the button is held.
    static GRAB: Cell<Option<i32>> = const { Cell::new(None) };
    /// The newest position a drag asked for and the loop has not read yet: a slow frame meets one request, not a queue.
    static ASKED: Cell<Option<i32>> = const { Cell::new(None) };
}

/// The bar's geometry: its track (client pixels), the page's height and the view's, and the position.
#[derive(Clone, Copy, Default)]
pub(super) struct Bar {
    pub track: RECT,
    pub content: i32,
    pub view: i32,
    pub scroll: i32,
    pub min_thumb: i32,
    /// The visible thumb's width: narrower than the track's hit area, centred in it.
    pub width: i32,
}

impl Bar {
    pub fn new(track: RECT, content: i32, view: i32, scroll: i32, scale: f64) -> Self {
        Self {
            track,
            content,
            view,
            scroll,
            min_thumb: (MIN_THUMB as f64 * scale).round() as i32,
            width: (THUMB_W as f64 * scale).round() as i32,
        }
    }

    /// How far the page can scroll.
    pub fn max(&self) -> i32 {
        (self.content - self.view).max(0)
    }

    /// Whether there is anything to scroll (otherwise the gutter stays empty).
    pub fn scrolls(&self) -> bool {
        self.max() > 0 && self.track.bottom > self.track.top
    }

    fn length(&self) -> i32 {
        self.track.bottom - self.track.top
    }

    /// The thumb's top and length, measured from the track's top: its length is the view's share of the page
    /// (never under `min_thumb`), and it travels the rest of the track as the page travels its range.
    pub fn thumb(&self) -> (i32, i32) {
        let (track, max) = (self.length().max(0), self.max());
        if max == 0 {
            return (0, track);
        }
        let share = i64::from(track) * i64::from(self.view) / i64::from(self.content);
        let length = (share as i32).max(self.min_thumb).min(track);
        let travel = i64::from(track - length);
        let top =
            (travel * i64::from(self.scroll.clamp(0, max)) + i64::from(max) / 2) / i64::from(max);
        (top as i32, length)
    }

    /// The position for a thumb whose top is `top` pixels below the track's top (the drag's inverse of `thumb`).
    pub fn scroll_for(&self, top: i32) -> i32 {
        let travel = self.length() - self.thumb().1;
        if travel <= 0 {
            return 0;
        }
        let max = i64::from(self.max());
        ((i64::from(top.clamp(0, travel)) * max + i64::from(travel) / 2) / i64::from(travel)) as i32
    }

    fn holds(&self, x: i32, y: i32) -> bool {
        (self.track.left..self.track.right).contains(&x)
            && (self.track.top..self.track.bottom).contains(&y)
    }

    /// The thumb as drawn: a capsule centred in the track.
    pub fn thumb_rect(&self) -> RECT {
        let (top, length) = self.thumb();
        let inset = ((self.track.right - self.track.left - self.width) / 2).max(0);
        RECT {
            left: self.track.left + inset,
            right: self.track.right - inset,
            top: self.track.top + top,
            bottom: self.track.top + top + length,
        }
    }
}

/// How far one wheel notch (120 units) scrolls, in pixels: Windows' lines per notch, or a page when it says so.
pub(super) fn notch_px(lines: u32, scale: f64, view: i32) -> i32 {
    if lines == u32::MAX {
        return page_px(view, scale);
    }
    let line = (LINE as f64 * scale).round() as i32;
    lines.min(100) as i32 * line
}

/// A page: the view less two lines (never under one).
pub(super) fn page_px(view: i32, scale: f64) -> i32 {
    let line = (LINE as f64 * scale).round() as i32;
    (view - 2 * line).max(line)
}

/// Where a wheel turn of `delta` units (positive is up) takes the page from `offset`, carrying `remainder` (in
/// 120ths of a pixel) so fine touchpad deltas never vanish. A turn the other way drops what was carried, and
/// clamping at either end drops the leftover, so the first turn back moves at once.
pub(super) fn wheel_step(
    offset: i32,
    delta: i32,
    notch: i32,
    max: i32,
    remainder: i32,
) -> (i32, i32) {
    let moving = -delta;
    let carry = if remainder != 0 && (remainder > 0) != (moving > 0) {
        0
    } else {
        remainder
    };
    let total = carry + moving * notch;
    let next = offset + total / 120;
    let clamped = next.clamp(0, max);
    let rest = if clamped == next { total % 120 } else { 0 };
    (clamped, rest)
}

/// Windows' wheel setting: lines per notch (3 unless the user changed it; `u32::MAX` is a page).
fn wheel_lines() -> u32 {
    let mut lines = 3u32;
    // SAFETY: fills a local the size the query declares.
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            Some((&mut lines as *mut u32).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    lines
}

fn post(window: HWND, code: usize, amount: i32) {
    // SAFETY: a plain posted message to our own window; the host loop reads it with the others.
    let _ = unsafe { PostMessageW(Some(window), SCROLL, WPARAM(code), LPARAM(amount as isize)) };
}

fn ask_drag(window: HWND, bar: &Bar, y: i32, grab: i32) {
    let asked = bar.scroll_for(y - bar.track.top - grab);
    if ASKED.replace(Some(asked)).is_none() {
        post(window, DRAG, 0);
    }
}

/// The bar's mouse use, from the dialog procedure (which cannot reach the window's state): a press on the thumb
/// grabs it, with the mouse captured so the drag goes on outside the window; a press on the track pages;
/// moves while grabbed post the position the thumb now stands for. True when the message was the bar's.
pub(super) fn mouse(window: HWND, message: u32, lparam: LPARAM, bar: &Bar) -> bool {
    let (x, y) = (
        i32::from(lparam.0 as i16),
        i32::from((lparam.0 >> 16) as i16),
    );
    match message {
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK if bar.scrolls() && bar.holds(x, y) => {
            let (top, length) = bar.thumb();
            let at = y - bar.track.top;
            if (top..top + length).contains(&at) {
                GRAB.set(Some(at - top));
                // SAFETY: our own live window takes the capture; it is released on the button's release.
                unsafe { SetCapture(window) };
            } else {
                post(window, PAGE, if at < top { -1 } else { 1 });
            }
            true
        }
        WM_MOUSEMOVE => GRAB.get().is_some_and(|grab| {
            ask_drag(window, bar, y, grab);
            true
        }),
        WM_LBUTTONUP => GRAB.take().is_some_and(|grab| {
            ask_drag(window, bar, y, grab);
            // SAFETY: gives back the capture this window took above.
            let _ = unsafe { ReleaseCapture() };
            true
        }),
        WM_CANCELMODE | WM_CAPTURECHANGED => {
            let held = GRAB.take().is_some();
            if held {
                ASKED.set(None);
            }
            if held && message != WM_CAPTURECHANGED {
                // SAFETY: gives back the capture this window took above.
                let _ = unsafe { ReleaseCapture() };
            }
            held
        }
        _ => false,
    }
}

/// Lets go of the thumb (a page, a DPI or a focus change ends a drag in progress).
pub(super) fn release() {
    ASKED.set(None);
    if GRAB.take().is_some() {
        // SAFETY: gives back a capture this thread's window may hold; harmless when it does not.
        let _ = unsafe { ReleaseCapture() };
    }
}

/// Draws the bar: a faint track and the thumb capsule (only when the page overflows).
pub(super) fn draw_bar(b: &GdiBatch<'_>, bar: &Bar, track: u32, thumb: u32) {
    if !bar.scrolls() {
        return;
    }
    let width = bar.width;
    let inset = ((bar.track.right - bar.track.left - width) / 2).max(0);
    let lane = RECT {
        left: bar.track.left + inset,
        right: bar.track.right - inset,
        ..bar.track
    };
    b.round(lane, width / 2, swap(track), swap(track));
    let rect = bar.thumb_rect();
    b.round(rect, width / 2, swap(thumb), swap(thumb));
}

impl SettingsWindow {
    pub(super) fn scrolling_body(&self) -> HWND {
        if self.search.is_open() {
            self.search.body
        } else {
            self.contents[self.page]
        }
    }
    /// A scroll request from the dialog (see the codes above): the wheel, a track click and the thumb all land
    /// on the offset at once.
    pub(super) fn scroll_message(&mut self, code: usize, amount: i32) -> Result<()> {
        let Some(scale) = self.font.as_ref().map(|f| f.scale) else {
            return Ok(());
        };
        let view = self.frame.viewport.bottom - self.frame.viewport.top;
        match code {
            PAGE => self.jump(self.scroll + amount * page_px(view, scale)),
            DRAG => match ASKED.take() {
                Some(asked) => self.jump(asked),
                None => Ok(()),
            },
            _ => {
                let notch = notch_px(wheel_lines(), scale, view);
                let (next, rest) = wheel_step(
                    self.scroll,
                    amount,
                    notch,
                    self.scroll_max,
                    self.wheel_remainder,
                );
                self.jump_to(next)?;
                self.wheel_remainder = rest;
                Ok(())
            }
        }
    }

    /// Scrolls to `to` (clamped), forgetting any carried wheel fraction: a drag, a page, a focused row coming
    /// into view.
    pub(super) fn jump(&mut self, to: i32) -> Result<()> {
        self.wheel_remainder = 0;
        self.jump_to(to)
    }

    /// Moves the page to `to`: one move of the content window (the system clips it to the viewport and repaints
    /// the strip it uncovers) and the thumb. Nothing else: no layout, no controls, no settings.
    fn jump_to(&mut self, to: i32) -> Result<()> {
        let to = to.clamp(0, self.scroll_max);
        if to == self.scroll {
            return Ok(());
        }
        // SAFETY: our own live content window.
        unsafe {
            SetWindowPos(
                self.scrolling_body(),
                None,
                0,
                -to,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )?;
        }
        self.scroll = to;
        let track = PAINT.with(|cell| {
            cell.borrow_mut().as_mut().map(|p| {
                p.bar.scroll = to;
                p.bar.track
            })
        });
        if let Some(track) = track {
            // SAFETY: our own live dialog; only the bar's strip is invalidated.
            let _ = unsafe { InvalidateRect(Some(self.window), Some(&track), true) };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(content: i32, view: i32, scroll: i32) -> Bar {
        Bar {
            track: RECT {
                left: 600,
                top: 0,
                right: 614,
                bottom: view,
            },
            content,
            view,
            scroll,
            min_thumb: 28,
            width: 6,
        }
    }

    #[test]
    fn drag_release_keeps_the_final_position_until_it_is_read() {
        let window = HWND::default();
        let bar = bar(1000, 500, 0);
        let position = |y: i32| LPARAM((y as isize) << 16);
        GRAB.set(Some(10));
        ASKED.set(Some(20));
        assert!(mouse(window, WM_MOUSEMOVE, position(130), &bar));
        assert!(mouse(window, WM_LBUTTONUP, position(260), &bar));
        assert!(!mouse(window, WM_CAPTURECHANGED, LPARAM(0), &bar));
        assert_eq!(ASKED.take(), Some(bar.max()));
    }

    #[test]
    fn the_thumb_is_the_views_share_and_fills_the_track_at_both_ends() {
        let (top, length) = bar(1000, 500, 0).thumb();
        assert_eq!((top, length), (0, 250), "half the page is in view");
        let (top, length) = bar(1000, 500, 500).thumb();
        assert_eq!(top + length, 500, "at the end it rests on the track's end");
        assert_eq!(
            bar(1000, 500, 250).thumb().0,
            125,
            "the middle is the middle"
        );
        assert_eq!(
            bar(1000, 500, -40).thumb().0,
            0,
            "a stale position is clamped"
        );
        assert_eq!(bar(1000, 500, 9999).thumb().0 + 250, 500);
    }

    #[test]
    fn a_long_page_keeps_a_grabbable_thumb() {
        let (top, length) = bar(100_000, 500, 0).thumb();
        assert_eq!((top, length), (0, 28));
        let (top, length) = bar(100_000, 500, 99_500).thumb();
        assert_eq!(top + length, 500);
        assert_eq!(
            bar(100_000, 10, 0).thumb().1,
            10,
            "never longer than the track"
        );
    }

    #[test]
    fn a_page_that_fits_has_no_bar() {
        assert!(!bar(500, 500, 0).scrolls());
        assert!(!bar(300, 500, 0).scrolls());
        assert!(bar(501, 500, 0).scrolls());
        assert_eq!(bar(500, 500, 0).scroll_for(50), 0);
    }

    #[test]
    fn dragging_the_thumb_maps_back_to_the_position() {
        let b = bar(1700, 500, 0);
        let max = b.max();
        let (_, length) = b.thumb();
        let travel = 500 - length;
        assert_eq!(b.scroll_for(0), 0);
        assert_eq!(
            b.scroll_for(travel),
            max,
            "the end of the track is the end of the page"
        );
        assert_eq!(b.scroll_for(-80), 0, "past the top stays at the top");
        assert_eq!(
            b.scroll_for(travel + 80),
            max,
            "past the bottom stays at the bottom"
        );
        let mut last = 0;
        for top in 0..=travel {
            let at = b.scroll_for(top);
            assert!(at >= last && at <= max, "monotonic and inside the page");
            last = at;
        }
        // Dragging to where the thumb stands now leaves the page where it is (to a pixel of the thumb's travel).
        for scroll in (0..=max).step_by(7) {
            let mut at = b;
            at.scroll = scroll;
            let back = at.scroll_for(at.thumb().0);
            assert!(
                (back - scroll).abs() <= max / travel + 1,
                "{scroll} came back as {back}"
            );
        }
    }

    #[test]
    fn the_mouse_finds_the_gutter_only_and_the_thumb_is_centred_in_it() {
        let b = bar(1000, 500, 0);
        assert!(b.holds(600, 0) && b.holds(613, 499));
        assert!(!b.holds(599, 10) && !b.holds(614, 10) && !b.holds(610, 500));
        let thumb = b.thumb_rect();
        assert_eq!(
            (thumb.left, thumb.right),
            (604, 610),
            "6 wide in the 14 wide track"
        );
        assert_eq!((thumb.top, thumb.bottom), (0, 250));
    }

    #[test]
    fn a_notch_follows_the_windows_wheel_setting() {
        // Three lines of 18 is the default notch; a page when Windows says so; capped, never zero-sized.
        assert_eq!(notch_px(3, 1.0, 500), 54);
        assert_eq!(notch_px(1, 2.0, 500), 36);
        assert_eq!(notch_px(u32::MAX, 1.0, 500), 464, "view less 36");
        assert_eq!(page_px(40, 1.0), 18, "never under one line");
        assert_eq!(
            notch_px(u32::MAX - 1, 1.0, 500),
            1800,
            "a silly value is capped"
        );
    }

    #[test]
    fn the_wheel_moves_and_stops_at_both_ends() {
        // One notch up from 100 is a notch up; the page's two ends hold.
        assert_eq!(wheel_step(100, 120, 54, 400, 0), (46, 0));
        assert_eq!(wheel_step(100, -120, 54, 400, 0), (154, 0));
        assert_eq!(wheel_step(10, 120, 54, 400, 0), (0, 0));
        assert_eq!(wheel_step(390, -240, 54, 400, 0), (400, 0));
        // Pressing on at an end moves nothing and carries nothing.
        assert_eq!(wheel_step(0, 30, 54, 400, 0), (0, 0));
        assert_eq!(wheel_step(400, -30, 54, 400, 0), (400, 0));
    }

    #[test]
    fn a_fine_wheel_moves_in_proportion_and_carries_what_is_left() {
        assert_eq!(wheel_step(100, -30, 54, 400, 0), (113, 60));
        // Tiny deltas add up across messages instead of vanishing: 54 * 2 / 120 is under a pixel each.
        let mut state = (100, 0);
        for _ in 0..10 {
            state = wheel_step(state.0, -2, 54, 400, state.1);
        }
        assert_eq!(state.0, 109, "ten small turns are 1080/120 pixels");
        assert_eq!(state.1, 0);
    }

    #[test]
    fn turning_back_drops_what_was_carried_and_moves_at_once() {
        let (at, rest) = wheel_step(100, -2, 54, 400, 0);
        assert_eq!((at, rest), (100, 108), "almost a pixel down is carried");
        // A turn back is its own 162/120: one pixel up, 42 over. The carried 108 cancels nothing of it.
        let (back, rest) = wheel_step(at, 3, 54, 400, rest);
        assert_eq!((back, rest), (99, -42));
        // Clamping at an end clears the carry, so the first turn away from it is not eaten.
        let (end, rest) = wheel_step(399, -30, 54, 400, 0);
        assert_eq!((end, rest), (400, 0));
        assert_eq!(wheel_step(end, 120, 54, 400, rest), (346, 0));
    }

    #[test]
    fn a_track_click_pages_by_the_view() {
        assert_eq!(page_px(500, 1.0), 464);
        assert_eq!(page_px(500, 1.0).clamp(0, 400), 400);
    }
}
