//! Scrolling the Settings window: the bar in the window's right gutter (drawn with the page, in one paint, so it
//! never flickers against it), its thumb drag and track paging, the wheel glide on the one motion clock, and the
//! cheap scroll step that moves the page's controls without measuring or re-creating anything.
//!
//! The old bar was the system's, painted over by hand: its thumb drag ran Windows' own modal loop, where the
//! posted scroll messages are never read (the page did not move), and every `SetScrollInfo` redrew it plain
//! before the overdraw. Nothing here runs unless a scroll message or a glide frame asks for it.

use super::*;
use crate::motion;
use std::{
    cell::Cell,
    time::{Duration, Instant},
};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};

/// `SCROLL` message codes (the wheel's is `dialog::WHEEL`); the amount rides in `lParam`.
const PAGE: usize = 1;
const DRAG: usize = 2;
/// A wheel notch (120 units) scrolls this many DIPs.
const NOTCH: i32 = 48;
/// A wheel step or a page glides this long (at the chosen speed).
const GLIDE: Duration = Duration::from_millis(180);
/// The shortest thumb, in DIPs, so it stays easy to grab on a long page.
const MIN_THUMB: i32 = 28;

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
}

impl Bar {
    pub fn new(track: RECT, content: i32, view: i32, scroll: i32, scale: f64) -> Self {
        Self {
            track,
            content,
            view,
            scroll,
            min_thumb: (MIN_THUMB as f64 * scale).round() as i32,
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

    /// The thumb as drawn: a capsule a third in from each side of the gutter.
    pub fn thumb_rect(&self) -> RECT {
        let (top, length) = self.thumb();
        let inset = ((self.track.right - self.track.left) / 3).max(2);
        RECT {
            left: self.track.left + inset,
            right: self.track.right - inset,
            top: self.track.top + top,
            bottom: self.track.top + top + length,
        }
    }
}

/// Where a wheel turn of `delta` units (positive is up) takes a glide aimed at `aim`.
fn wheel_aim(aim: i32, delta: i32, notch: i32, max: i32) -> i32 {
    (aim - delta * notch / 120).clamp(0, max)
}

/// Where a track click (`direction` -1 above the thumb, 1 below) takes a glide aimed at `aim`: one view.
fn page_aim(aim: i32, view: i32, direction: i32, max: i32) -> i32 {
    (aim + direction * view).clamp(0, max)
}

fn post(window: HWND, code: usize, amount: i32) {
    // SAFETY: a plain posted message to our own window; the host loop reads it with the others.
    let _ = unsafe { PostMessageW(Some(window), SCROLL, WPARAM(code), LPARAM(amount as isize)) };
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
            let asked = bar.scroll_for(y - bar.track.top - grab);
            if ASKED.replace(Some(asked)).is_none() {
                post(window, DRAG, asked);
            }
            true
        }),
        WM_LBUTTONUP | WM_CANCELMODE | WM_CAPTURECHANGED => {
            let held = GRAB.take().is_some();
            if held && message != WM_CAPTURECHANGED {
                // SAFETY: gives back the capture this window took above.
                let _ = unsafe { ReleaseCapture() };
            }
            held
        }
        _ => false,
    }
}

impl Paint {
    /// Moves everything the page paints (not the bar) by `dy`.
    fn shift(&mut self, dy: i32) {
        let by = |r: &mut RECT| {
            r.top += dy;
            r.bottom += dy;
        };
        by(&mut self.title.0);
        self.labels.iter_mut().for_each(|(r, _)| by(r));
        self.cards.iter_mut().for_each(by);
        self.fields.iter_mut().for_each(by);
    }
}

/// Draws the thumb (its fill is the track's: only the capsule shows).
pub(super) unsafe fn draw_thumb(dc: HDC, bar: &Bar, colour: u32) {
    if !bar.scrolls() {
        return;
    }
    let thumb = bar.thumb_rect();
    // SAFETY: the caller's live DC.
    unsafe { round(dc, thumb, (thumb.right - thumb.left) / 2, colour, colour) };
}

impl SettingsWindow {
    /// A scroll request from the dialog (see the codes above): the wheel and the track glide, the thumb follows.
    pub(super) fn scroll_message(&mut self, code: usize, amount: i32) -> Result<()> {
        let Some(scale) = self.font.as_ref().map(|f| f.scale) else {
            return Ok(());
        };
        let aim = self.glide.target() as i32;
        match code {
            PAGE => {
                self.glide_to(page_aim(aim, self.view(scale), amount, self.scroll_max));
                Ok(())
            }
            DRAG => self.jump(ASKED.take().unwrap_or(amount)),
            _ => {
                let notch = (NOTCH as f64 * scale).round() as i32;
                self.glide_to(wheel_aim(aim, amount, notch, self.scroll_max));
                Ok(())
            }
        }
    }

    /// The view's height: the client above the footer.
    fn view(&self, scale: f64) -> i32 {
        let mut client = RECT::default();
        // SAFETY: our own live window.
        let _ = unsafe { GetClientRect(self.window, &mut client) };
        (client.bottom - (FOOT as f64 * scale).round() as i32).max(1)
    }

    /// Aims the glide at `to` (kept inside the page): the host's frames carry it there, and none run after.
    fn glide_to(&mut self, to: i32) {
        self.glide.retarget(
            to.clamp(0, self.scroll_max) as f32,
            motion::time(GLIDE),
            motion::SETTLE,
        );
    }

    /// Scrolls to `to` at once (a drag, a focused row coming into view) and ends any glide.
    pub(super) fn jump(&mut self, to: i32) -> Result<()> {
        let to = to.clamp(0, self.scroll_max);
        self.glide = motion::Tween::rest(to as f32);
        self.step(to)
    }

    /// One frame of the glide; true while it runs.
    pub fn glide_frame(&mut self, now: Instant) -> Result<bool> {
        let running = self.glide.running(now);
        self.step((self.glide.value(now).round() as i32).clamp(0, self.scroll_max))?;
        Ok(running)
    }

    /// Moves the page to `to`: the controls as one batch, the painted cards and the thumb with them, and one
    /// repaint of the pane (never the sidebar or the footer). No text is measured; `place` made the list.
    fn step(&mut self, to: i32) -> Result<()> {
        let dy = to - self.scroll;
        let Some(scale) = self.font.as_ref().map(|f| f.scale) else {
            return Ok(());
        };
        if dy == 0 {
            return Ok(());
        }
        self.scroll = to;
        let mut client = RECT::default();
        // SAFETY: our own live window, its children and a deferred batch that ends before returning.
        unsafe {
            GetClientRect(self.window, &mut client)?;
            let view = client.bottom - (FOOT as f64 * scale).round() as i32;
            let mut moves = self.moves.borrow_mut();
            let mut batch = BeginDeferWindowPos(moves.len().max(1) as i32)?;
            for it in moves.iter_mut() {
                it.y -= dy;
                batch = DeferWindowPos(
                    batch,
                    it.h,
                    None,
                    it.x,
                    it.y,
                    it.w,
                    it.ht,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS,
                )?;
            }
            EndDeferWindowPos(batch)?;
            for it in moves.iter() {
                if it.boxed {
                    inset(it.h, it.w, it.cut, it.y, view);
                } else {
                    clip(it.h, it.y, it.ht, it.w, view);
                }
            }
            drop(moves);
            PAINT.with(|cell| {
                if let Some(p) = cell.borrow_mut().as_mut() {
                    p.shift(-dy);
                    p.bar.scroll = to;
                }
            });
            let pane = RECT {
                left: (SIDE_W as f64 * scale).round() as i32,
                top: 0,
                right: client.right,
                bottom: view,
            };
            let _ = RedrawWindow(
                Some(self.window),
                Some(&pane),
                None,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
            );
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
                right: 617,
                bottom: view,
            },
            content,
            view,
            scroll,
            min_thumb: 28,
        }
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
    fn the_mouse_finds_the_gutter_only() {
        let b = bar(1000, 500, 0);
        assert!(b.holds(600, 0) && b.holds(616, 499));
        assert!(!b.holds(599, 10) && !b.holds(617, 10) && !b.holds(610, 500));
        let thumb = b.thumb_rect();
        assert!(
            thumb.left > 600 && thumb.right < 617,
            "a capsule inside the gutter"
        );
        assert_eq!((thumb.top, thumb.bottom), (0, 250));
    }

    #[test]
    fn the_wheel_and_the_track_aim_inside_the_page() {
        // One notch up from 100 is 48 up; the page's two ends hold.
        assert_eq!(wheel_aim(100, 120, 48, 400), 52);
        assert_eq!(wheel_aim(100, -120, 48, 400), 148);
        assert_eq!(wheel_aim(10, 120, 48, 400), 0);
        assert_eq!(wheel_aim(390, -240, 48, 400), 400);
        // A fine wheel (touchpad) moves in proportion instead of a whole notch per message.
        assert_eq!(wheel_aim(100, -30, 48, 400), 112);
        assert_eq!(page_aim(100, 500, 1, 400), 400);
        assert_eq!(page_aim(300, 500, -1, 400), 0);
        assert_eq!(page_aim(0, 100, 1, 400), 100);
    }
}
