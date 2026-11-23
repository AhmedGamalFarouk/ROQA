//! The screenshot pill's stack (design `cook/design/moon-and-screenshot-pill.md`, part 2): up to three shots,
//! newest first, held in memory only. The stack shows on the notice row whenever `Power` has no notice; its
//! hold is one host deadline, absent while the pill is hovered or dragged. Nothing here touches the disk.

use crate::{
    icons::IconPixels,
    motion::{self, Tween},
};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

/// Shots the pill keeps; a fourth drops the oldest (Windows still has it).
pub const MAX: usize = 3;
/// How long the pill holds once it shows, and again after the pointer leaves it.
pub const HOLD: Duration = Duration::from_secs(8);
/// The thumbnail's pixels: the 64 x 36 DIP slot at up to 300 % scale.
pub const THUMB: (u32, u32) = (192, 108);

/// One screenshot: the clipboard's `CF_DIB` bytes as copied, and its thumbnail.
pub struct Shot {
    /// Unique for this run: keys the thumbnail's device bitmap.
    pub id: u64,
    pub dib: Vec<u8>,
    pub thumb: IconPixels,
    /// The local time it was taken, "HH-MM", for the dragged file's name.
    pub time: String,
}

pub struct Shots {
    list: Vec<Rc<Shot>>,
    showing: bool,
    held: bool,
    until: Option<Instant>,
    /// The newest thumbnail lands (pop) and the text fades in, 0 to 1.
    pub land: Tween,
    /// The pointer is on the chip, 0 to 1 (settle).
    pub hover: Tween,
    /// A shot is being dragged out (the system's drag loop runs).
    pub dragging: bool,
}

impl Default for Shots {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            showing: false,
            held: false,
            until: None,
            land: Tween::rest(0.0),
            hover: Tween::rest(0.0),
            dragging: false,
        }
    }
}

impl Shots {
    pub fn list(&self) -> &[Rc<Shot>] {
        &self.list
    }
    /// The pill is on the row now.
    pub fn showing(&self) -> bool {
        self.showing
    }
    /// The pill shows and is hovered or dragged: nothing takes the row.
    pub fn held(&self) -> bool {
        self.showing && self.held
    }
    /// A new shot goes on top; a showing pill restarts its hold.
    pub fn add(&mut self, shot: Rc<Shot>, now: Instant) {
        // The Snipping Tool writes the clipboard twice for one shot: the same image again is not a new one.
        if self.list.first().is_some_and(|top| top.dib == shot.dib) {
            return;
        }
        self.list.insert(0, shot);
        self.list.truncate(MAX);
        if self.showing {
            self.landing();
            if !self.held {
                self.until = Some(now + HOLD);
            }
        }
    }
    /// The row is `free` (no notice): the stack shows, with a fresh hold; true when the pill came or went.
    pub fn sync(&mut self, free: bool, now: Instant) -> bool {
        let show = free && !self.list.is_empty();
        if show == self.showing {
            return false;
        }
        self.showing = show;
        self.until = (show && !self.held).then(|| now + HOLD);
        if show {
            self.landing();
        }
        true
    }
    fn landing(&mut self) {
        self.land = Tween::rest(0.0);
        self.land.retarget(1.0, motion::POP_TIME, motion::POP);
    }
    /// The pointer came onto the chip or left it: the hover wash follows, and the hold with it.
    pub fn point(&mut self, over: bool, now: Instant) -> bool {
        let to = if over { 1.0 } else { 0.0 };
        if self.hover.target() == to {
            return false;
        }
        self.hover
            .retarget(to, motion::HOVER_IN_TIME, motion::SETTLE);
        self.hold(over || self.dragging, now);
        true
    }
    /// A tween runs: the notch keeps drawing frames.
    pub fn running(&self, now: Instant) -> bool {
        self.land.running(now) || self.hover.running(now)
    }
    /// Hovered or dragged (`held`): the hold is absent; leaving starts the full hold again.
    pub fn hold(&mut self, held: bool, now: Instant) -> bool {
        if held == self.held {
            return false;
        }
        self.held = held;
        if self.showing {
            self.until = (!held).then(|| now + HOLD);
        }
        true
    }
    /// True once when the hold ran out: the pill leaves for good and its shots are freed.
    pub fn over(&mut self, now: Instant) -> bool {
        if !self.showing || self.until.is_none_or(|until| now < until) {
            return false;
        }
        self.list.clear();
        self.showing = false;
        self.until = None;
        true
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.until.filter(|_| self.showing)
    }
    /// Takes the shot `id` off the stack (it was dropped somewhere); true when it was there.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.list.len();
        self.list.retain(|shot| shot.id != id);
        self.list.len() != before
    }
}

/// Where a `CF_DIB`'s pixels are: 24 or 32 bits, uncompressed or plain bit fields.
#[derive(Debug, PartialEq, Eq)]
pub struct Dib {
    pub width: usize,
    pub height: usize,
    bottom_up: bool,
    /// Bytes per pixel, 3 or 4, blue first.
    pub bytes: usize,
    stride: usize,
    offset: usize,
}

impl Dib {
    /// The bytes of row `y`, counted from the top.
    pub fn row(&self, y: usize) -> std::ops::Range<usize> {
        let row = if self.bottom_up {
            self.height - 1 - y
        } else {
            y
        };
        let start = self.offset + row * self.stride;
        start..start + self.width * self.bytes
    }
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// Reads a `CF_DIB` header; `None` for a format a screenshot never has or bytes too short for its rows.
pub fn dib(data: &[u8]) -> Option<Dib> {
    let size = u32_at(data, 0)? as usize;
    let width = u32_at(data, 4)? as i32;
    let height = u32_at(data, 8)? as i32;
    let bits = u16::from_le_bytes(data.get(14..16)?.try_into().ok()?);
    let compression = u32_at(data, 16)?;
    let colours = u32_at(data, 32)? as usize;
    if size < 40 || width <= 0 || height == 0 || !matches!(bits, 24 | 32) {
        return None;
    }
    // BI_RGB, or BI_BITFIELDS whose three masks follow a short header.
    let masks = match compression {
        0 => 0,
        3 if size == 40 => 12,
        3 => 0,
        _ => return None,
    };
    let (width, rows) = (width as usize, height.unsigned_abs() as usize);
    let bytes = usize::from(bits / 8);
    let stride = (width * bytes).div_ceil(4) * 4;
    let offset = size + masks + colours * 4;
    (data.len() >= offset + stride.checked_mul(rows)?).then_some(Dib {
        width,
        height: rows,
        bottom_up: height > 0,
        bytes,
        stride,
        offset,
    })
}

/// The thumbnail: the image centre-cropped to the slot's 16:9 and scaled to `THUMB`, opaque, four samples a pixel.
pub fn thumb(data: &[u8], dib: &Dib) -> IconPixels {
    let (tw, th) = (THUMB.0 as usize, THUMB.1 as usize);
    // The largest centred 16:9 crop.
    let (cw, ch) = if dib.width * th > dib.height * tw {
        (dib.height * tw / th, dib.height)
    } else {
        (dib.width, dib.width * th / tw)
    };
    let (cw, ch) = (cw.max(1), ch.max(1));
    let (x0, y0) = ((dib.width - cw) / 2, (dib.height - ch) / 2);
    let pixel = |x: usize, y: usize| {
        let at = dib.row(y).start + x * dib.bytes;
        [data[at], data[at + 1], data[at + 2]]
    };
    let mut pixels = Vec::with_capacity(tw * th * 4);
    for y in 0..th {
        for x in 0..tw {
            let mut sum = [0u32; 3];
            for (sx, sy) in [(1, 1), (3, 1), (1, 3), (3, 3)] {
                let px = x0 + ((x * 4 + sx) * cw / (tw * 4)).min(cw - 1);
                let py = y0 + ((y * 4 + sy) * ch / (th * 4)).min(ch - 1);
                for (total, value) in sum.iter_mut().zip(pixel(px, py)) {
                    *total += u32::from(value);
                }
            }
            pixels.extend(sum.map(|total| (total / 4) as u8));
            pixels.push(255);
        }
    }
    IconPixels {
        width: THUMB.0,
        height: THUMB.1,
        pixels,
    }
}

/// The pill's two lines: the count, then what to do (`dragging` while it is being dragged).
pub fn lines(count: usize, dragging: bool) -> (String, &'static str) {
    let first = if count > 1 {
        format!("{count} screenshots")
    } else {
        "Screenshot".to_owned()
    };
    let second = if dragging {
        "Drop to keep or share"
    } else if count > 1 {
        "Drag the newest"
    } else {
        "Drag it anywhere"
    };
    (first, second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::{Extra, Power};

    fn shot(id: u64) -> Rc<Shot> {
        Rc::new(Shot {
            id,
            dib: id.to_le_bytes().to_vec(),
            thumb: IconPixels {
                width: 0,
                height: 0,
                pixels: Vec::new(),
            },
            time: String::new(),
        })
    }

    /// The host's one pass: a notice ends, the pill takes a free row, then a waiting notice may show.
    fn pass(power: &mut Power, shots: &mut Shots, now: Instant) {
        power.over(now);
        shots.over(now);
        shots.sync(power.text().is_none(), now);
        power.set_shot(shots.showing().then(|| shots.held()));
        power.promote(now);
    }

    #[test]
    fn a_plain_notice_waits_for_the_shot_and_shows_after_it() {
        let (mut power, mut shots, now) = (Power::default(), Shots::default(), Instant::now());
        shots.add(shot(1), now);
        pass(&mut power, &mut shots, now);
        assert!(shots.showing());
        assert!(!power.announce("Water".into(), now, HOLD, Extra::None));
        let later = now + HOLD;
        pass(&mut power, &mut shots, later);
        assert!(!shots.showing() && shots.list().is_empty(), "the shot left");
        assert_eq!(power.text(), Some("Water"));
    }

    #[test]
    fn a_prayer_or_a_button_row_takes_the_row_and_the_shot_returns_with_a_fresh_hold() {
        let (mut power, mut shots, now) = (Power::default(), Shots::default(), Instant::now());
        shots.add(shot(1), now);
        pass(&mut power, &mut shots, now);
        power.yield_shot();
        assert!(power.announce("Fajr".into(), now, HOLD, Extra::None));
        pass(&mut power, &mut shots, now);
        assert!(
            !shots.showing() && shots.list().len() == 1,
            "waits, not lost"
        );
        let after = now + HOLD;
        pass(&mut power, &mut shots, after);
        assert!(shots.showing());
        assert_eq!(shots.deadline(), Some(after + HOLD));
        // A due reminder's Snooze row takes it the same way, without a yield.
        assert!(power.admits(after, HOLD, Extra::Snooze));
    }

    #[test]
    fn a_shot_waits_for_a_notice_already_showing() {
        let (mut power, mut shots, now) = (Power::default(), Shots::default(), Instant::now());
        assert!(power.announce("Build done".into(), now, HOLD, Extra::None));
        shots.add(shot(1), now);
        pass(&mut power, &mut shots, now);
        assert!(!shots.showing());
        pass(&mut power, &mut shots, now + HOLD);
        assert!(shots.showing() && power.text().is_none());
    }

    #[test]
    fn more_shots_merge_up_to_three_and_a_held_pill_keeps_the_row() {
        let (mut power, mut shots, now) = (Power::default(), Shots::default(), Instant::now());
        for id in 1..=4 {
            shots.add(shot(id), now);
        }
        let again = Rc::new(Shot {
            id: 9,
            dib: 4u64.to_le_bytes().to_vec(),
            ..Rc::into_inner(shot(0)).expect("one owner")
        });
        shots.add(again, now);
        let ids: Vec<u64> = shots.list().iter().map(|s| s.id).collect();
        assert_eq!(ids, [4, 3, 2], "newest first, the oldest dropped");
        pass(&mut power, &mut shots, now);
        shots.hold(true, now);
        pass(&mut power, &mut shots, now);
        assert_eq!(shots.deadline(), None, "absent while hovered");
        power.yield_shot();
        assert!(!power.admits(now, HOLD, Extra::Snooze), "nothing takes it");
        assert!(!shots.over(now + HOLD * 4));
        shots.hold(false, now);
        assert_eq!(
            shots.deadline(),
            Some(now + HOLD),
            "leaving restarts the hold"
        );
        assert!(shots.remove(4) && !shots.remove(4));
    }

    #[test]
    fn a_dib_header_finds_its_rows_and_the_thumb_is_a_centre_crop() {
        // 4 x 2, 32 bits, bottom-up: the bottom row is red, the top row blue.
        let mut data = vec![0u8; 40];
        data[0] = 40;
        data[4] = 4;
        data[8] = 2;
        data[14] = 32;
        for colour in [[0, 0, 255, 0], [255, 0, 0, 0]] {
            for _ in 0..4 {
                data.extend(colour);
            }
        }
        let info = dib(&data).expect("valid");
        assert_eq!((info.width, info.height), (4, 2));
        let pixels = thumb(&data, &info);
        assert_eq!(pixels.pixels.len(), (THUMB.0 * THUMB.1 * 4) as usize);
        assert_eq!(&pixels.pixels[..4], &[255, 0, 0, 255], "top row first");
        assert_eq!(dib(&data[..60]), None, "too short");
        assert_eq!(
            lines(2, false),
            ("2 screenshots".to_owned(), "Drag the newest")
        );
    }
}
