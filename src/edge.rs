//! Shared parts of the dock's volume and brightness popups and the dock's hover timing: the list rows and their
//! geometry, the wheel and level maths, the hover deadlines, and the brightness write throttle. The popups
//! themselves are in `host/levels.rs`, drawn by `dock_menu`.
use crate::{dock::px, layout::Bounds};
use std::time::{Duration, Instant};

/// How long the dock's tuck hides after the pointer leaves.
pub const HIDE_DELAY: Duration = Duration::from_millis(800);
/// Brightness writes go out at most this often (about 5 a second); the latest value wins.
pub const WRITE_GAP: Duration = Duration::from_millis(200);
/// A brightness reading that has not come back this long after the popup showed means "not supported"
/// (a hung DDC call would otherwise leave an empty row).
pub const READ_LIMIT: Duration = Duration::from_secs(2);

// Geometry in DIPs.
const MARGIN: i32 = 8;
const LIST_W: i32 = 250;
const ROW_H: i32 = 34;
const LIST_PAD: i32 = 6;
/// A mixer row's left end, in DIPs, where a click mutes the app instead of setting its level.
pub const MIX_ICON: i32 = 40;
/// A mixer row's text is cut this far from its right end, in DIPs, for the percent.
pub const MIX_PERCENT: i32 = 48;
/// Percent per wheel notch, and with Shift held.
const WHEEL_STEP: i32 = 3;
const FINE_STEP: i32 = 1;
const WHEEL_DELTA: i32 = 120;

/// One row of the device list, in window pixels.
pub struct ListRow<'a> {
    pub name: &'a str,
    pub current: bool,
    /// An icon drawn before the name (a folder stack's files, the folder members); none for plain rows.
    pub icon: Option<&'a crate::icons::IconPixels>,
    /// Keeps the icon's room while the icon is still on its way (a folder stack row).
    pub room: bool,
    /// A mixer row: the app's level and whether it is muted; drawn as a capsule slider.
    pub level: Option<(u8, bool)>,
    /// An AI limit row: its used percent, drawn as a thin bar under the text.
    pub bar: Option<u8>,
    /// The moon popup's hero (its lit share and lit limb); `name` holds its three lines.
    pub moon: Option<(f32, bool)>,
    /// A right-aligned value after the name (a moon date row).
    pub value: Option<&'a str>,
    pub rect: Bounds,
}

/// The device list window's size and its panel inside the transparent margin.
pub fn list_layout(dpi: u32, rows: usize) -> ((i32, i32), Bounds) {
    let margin = px(MARGIN, dpi);
    let panel = Bounds {
        x: margin,
        y: margin,
        width: px(LIST_W, dpi),
        height: rows as i32 * px(ROW_H, dpi) + 2 * px(LIST_PAD, dpi),
    };
    ((panel.width + 2 * margin, panel.height + 2 * margin), panel)
}

pub fn row_rect(dpi: u32, panel: Bounds, index: usize) -> Bounds {
    let pad = px(LIST_PAD, dpi);
    Bounds {
        x: panel.x + pad,
        y: panel.y + pad + index as i32 * px(ROW_H, dpi),
        width: panel.width - 2 * pad,
        height: px(ROW_H, dpi),
    }
}

/// The row under window point `(x, y)`.
pub fn row_at(dpi: u32, rows: usize, x: i32, y: i32) -> Option<usize> {
    row_in(dpi, list_layout(dpi, rows).1, rows, x, y)
}

/// The row of `panel` under window point `(x, y)`.
fn row_in(dpi: u32, panel: Bounds, rows: usize, x: i32, y: i32) -> Option<usize> {
    (0..rows).find(|&index| {
        let row = row_rect(dpi, panel, index);
        (row.x..row.x + row.width).contains(&x) && (row.y..row.y + row.height).contains(&y)
    })
}

/// The level a pointer at window column `x` means on a mixer row: 0 at its left end, 100 at its right.
pub fn mix_percent(x: i32, row: Bounds) -> u8 {
    let across = f64::from(x - row.x) / f64::from(row.width.max(1));
    (across * 100.0).round().clamp(0.0, 100.0) as u8
}

/// Wheel movement `delta` added to the remainder `acc` of earlier ones: whole notches, and what is left.
pub fn wheel_steps(acc: i32, delta: i32) -> (i32, i32) {
    let total = acc + delta;
    (total / WHEEL_DELTA, total % WHEEL_DELTA)
}

pub fn step_level(level: u8, steps: i32, fine: bool) -> u8 {
    let step = if fine { FINE_STEP } else { WHEEL_STEP };
    (i32::from(level) + steps * step).clamp(0, 100) as u8
}

// ---- Timing (pure) ---------------------------------------------------------------------------------

/// When something shows after the pointer arrives and hides after it leaves (never while held).
#[derive(Default)]
pub struct Hover {
    over: bool,
    shown: bool,
    held: bool,
    due: Option<Instant>,
}
impl Hover {
    /// `enter` with its own reveal delay (the dock's edge strip is quicker than the sliders').
    pub fn enter_after(&mut self, now: Instant, delay: Duration) {
        if !std::mem::replace(&mut self.over, true) {
            self.due = (!self.shown).then_some(now + delay);
        }
    }
    /// The pointer left what the pill revealed: a shown pill hides `delay` from now, not at the slower poll.
    pub fn release(&mut self, now: Instant, delay: Duration) {
        if self.shown && !self.over {
            self.due = Some(now + delay);
        }
    }
    pub fn leave(&mut self, now: Instant) {
        if std::mem::replace(&mut self.over, false) {
            self.due = self.shown.then_some(now + HIDE_DELAY);
        }
    }
    /// A hold has no deadline; its release event starts the user's normal hide delay.
    pub fn tick_after(&mut self, now: Instant, held: bool, delay: Duration) -> Option<bool> {
        if held {
            self.held = true;
            self.due = None;
            return (!std::mem::replace(&mut self.shown, true)).then_some(true);
        }
        if std::mem::replace(&mut self.held, false) && !self.over {
            self.due = Some(now + delay);
        }
        self.due.filter(|&due| now >= due)?;
        if !self.shown {
            self.due = None;
            self.shown = true;
            return Some(true);
        }
        self.due = None;
        self.shown = false;
        Some(false)
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.due
    }
    /// Shown by something other than the pointer (an alert): it hides `delay` from now unless held.
    pub fn show_until(&mut self, now: Instant, delay: Duration) {
        (self.over, self.shown, self.held, self.due) = (false, true, false, Some(now + delay));
    }
}

/// Coalesces brightness writes: the latest value waits for the gap since the last send.
#[derive(Default)]
pub struct Throttle {
    sent: Option<Instant>,
    latest: Option<u8>,
}
impl Throttle {
    pub fn push(&mut self, value: u8) {
        self.latest = Some(value);
    }
    /// Puts back a value `take` returned but the worker's queue refused; a newer value still wins.
    pub fn restore(&mut self, value: u8) {
        self.latest.get_or_insert(value);
    }
    /// The value to send now, if one waits and `WRITE_GAP` has passed since the last send.
    pub fn take(&mut self, now: Instant) -> Option<u8> {
        self.latest?;
        if self.sent.is_some_and(|sent| now < sent + WRITE_GAP) {
            return None;
        }
        self.sent = Some(now);
        self.latest.take()
    }
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.latest
            .map(|_| self.sent.map_or(now, |sent| sent + WRITE_GAP))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wheel_moves_three_percent_a_notch_one_with_shift_and_keeps_the_remainder() {
        assert_eq!(wheel_steps(0, 120), (1, 0));
        assert_eq!(wheel_steps(0, -240), (-2, 0));
        // High-resolution wheels add up to a notch.
        let (steps, rest) = wheel_steps(0, 30);
        assert_eq!((steps, rest), (0, 30));
        let (steps, rest) = wheel_steps(rest, 100);
        assert_eq!((steps, rest), (1, 10));
        assert_eq!(wheel_steps(-90, -60), (-1, -30));
        assert_eq!(
            (
                step_level(50, 1, false),
                step_level(98, 1, false),
                step_level(2, -1, false),
                step_level(0, -9, false),
                step_level(50, 1, true),
                step_level(50, -2, true)
            ),
            (53, 100, 0, 0, 51, 48)
        );
    }

    #[test]
    fn the_device_list_picks_the_row_under_the_pointer() {
        let (size, panel) = list_layout(96, 3);
        assert_eq!(size, (266, 3 * 34 + 12 + 16));
        let row = |i| row_rect(96, panel, i);
        assert_eq!(row_at(96, 3, row(1).x + 4, row(1).y + 4), Some(1));
        assert_eq!(
            row_at(96, 3, row(2).x + 4, row(2).y + row(2).height - 1),
            Some(2)
        );
        // The margin, the padding and below the last row are no row.
        assert_eq!(row_at(96, 3, 2, 2), None);
        assert_eq!(row_at(96, 3, row(0).x, row(0).y - 1), None);
        assert_eq!(row_at(96, 3, row(2).x + 4, row(2).y + row(2).height), None);
        assert_eq!(row_at(96, 0, 20, 20), None);
        // Rows never overlap and stay inside the panel.
        assert!(row(0).y + row(0).height == row(1).y);
        assert!(row(2).y + row(2).height <= panel.y + panel.height);
    }

    #[test]
    fn a_mixer_row_maps_the_pointer_column_to_a_level_and_always_has_a_row() {
        let (_, panel) = list_layout(96, 4);
        let row = row_rect(96, panel, 2);
        assert_eq!(mix_percent(row.x, row), 0);
        assert_eq!(mix_percent(row.x + row.width / 2, row), 50);
        assert_eq!(mix_percent(row.x + row.width, row), 100);
        // Outside the row clamps.
        assert_eq!(
            (
                mix_percent(row.x - 30, row),
                mix_percent(row.x + row.width + 30, row)
            ),
            (0, 100)
        );
    }

    #[test]
    fn brightness_writes_are_limited_to_five_a_second_and_the_latest_wins() {
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut throttle = Throttle::default();
        assert_eq!(
            (throttle.take(at(0)), throttle.deadline(at(0))),
            (None, None)
        );
        // The first value goes out at once.
        throttle.push(10);
        assert_eq!(throttle.deadline(at(0)), Some(at(0)));
        assert_eq!(throttle.take(at(0)), Some(10));
        // A burst inside the gap sends nothing and keeps only the newest value.
        for (ms, value) in [(10, 20), (50, 30), (120, 40), (199, 50)] {
            throttle.push(value);
            assert_eq!(throttle.take(at(ms)), None);
        }
        assert_eq!(throttle.deadline(at(199)), Some(at(200)));
        assert_eq!(throttle.take(at(200)), Some(50));
        // Nothing waiting, nothing sent, and the gap counts from the last send.
        assert_eq!(throttle.take(at(900)), None);
        throttle.push(60);
        assert_eq!(throttle.take(at(900)), Some(60));
        // Five sends fit in a second of constant movement.
        let mut sent = 0;
        let mut throttle = Throttle::default();
        for ms in (0..1000).step_by(10) {
            throttle.push((ms / 10) as u8);
            sent += usize::from(throttle.take(at(ms)).is_some());
        }
        assert_eq!(sent, 5);
    }

    #[test]
    fn a_refused_write_is_put_back_and_a_newer_value_still_wins() {
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut throttle = Throttle::default();
        // The last drag value is taken, the queue is full, so it goes back and is sent after the gap.
        throttle.push(70);
        assert_eq!(throttle.take(at(0)), Some(70));
        throttle.restore(70);
        assert_eq!(throttle.take(at(100)), None);
        assert_eq!(throttle.deadline(at(100)), Some(at(200)));
        assert_eq!(throttle.take(at(200)), Some(70));
        // A value pushed meanwhile is newer than the refused one and is not overwritten by it.
        throttle.push(80);
        throttle.restore(70);
        assert_eq!(throttle.take(at(400)), Some(80));
        assert_eq!(throttle.take(at(900)), None);
    }

    #[test]
    fn rows_are_found_in_the_panel_they_belong_to() {
        let panel = list_layout(96, 2).1;
        let second = row_rect(96, panel, 1);
        assert_eq!(row_in(96, panel, 2, second.x + 1, second.y + 1), Some(1));
        assert_eq!(
            row_in(96, panel, 2, panel.x + panel.width + 20, second.y),
            None
        );
        // The list's own lookup is the same one on the list's panel.
        let first = row_rect(96, list_layout(96, 3).1, 0);
        assert_eq!(row_at(96, 3, first.x, first.y), Some(0));
    }
}
