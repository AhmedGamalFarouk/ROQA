use super::*;
use crate::tabs::Tab;

const PLATE_HEIGHT: f32 = 36.0;
const INSET: f32 = 24.0;
const BUTTON_WIDTH: f32 = 92.0;
const COMPACT_WIDTH: f32 = 296.0;
const COMPACT_ROW_HEIGHT: f32 = 30.0;
static TOP: AtomicU16 = AtomicU16::new(0);

pub(super) fn grow_glance(from: NotchShape, shown: f32) -> NotchShape {
    if !active() {
        return from;
    }
    NotchShape {
        width: from.width + (COMPACT_WIDTH - from.width).max(0.0) * shown,
        height: from.height + 34.0 * shown,
        radius: from.radius + 17.0 * shown,
        ..from
    }
}

fn plate_top(page: Tab, track: bool, height: u16) -> u16 {
    let base = (NOTCH_EXPANDED.height as u16).max(height);
    if page == Tab::Media && track && height == 0 {
        return base + 4;
    }
    let last_target = match page {
        Tab::Prayer => 118,
        Tab::Weather => 116,
        _ => 0,
    };
    let gear = if track { base - 12 } else { 0 };
    (base - 8).max(last_target.max(gear) + 8)
}

pub fn set_notice_footer(page: Tab, track: bool, present: bool) {
    let top = if present {
        plate_top(page, track, PAGE_HEIGHT.load(Relaxed))
    } else {
        0
    };
    TOP.store(top, Relaxed);
}

pub(super) fn active() -> bool {
    TOP.load(Relaxed) != 0
}

pub(super) fn grown_height(base: f32, shown: f32) -> Option<f32> {
    let top = TOP.load(Relaxed);
    (top != 0).then(|| base + (f32::from(top) + PLATE_HEIGHT + 10.0 - base) * shown)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoticeFooter {
    pub plate: Bounds,
    pub text: Bounds,
    pub button: Bounds,
    pub compact: bool,
    body: NotchBody,
    radius: f32,
    dpi: u32,
}

impl NoticeFooter {
    pub fn current(
        window_width: i32,
        dpi: u32,
        progress: f32,
        track: bool,
        shown: f32,
    ) -> Option<Self> {
        let top = TOP.load(Relaxed);
        if top == 0 || shown <= 0.0 {
            return None;
        }
        let shape = notch_shape(progress, track);
        let body = notch_body(window_width, dpi, shape);
        let compact = motion::reveal(progress) == 0.0;
        let top = if compact {
            glance_row_top(track) + 14.0 * motion::smooth(progress / 0.35)
        } else {
            f32::from(top) - 65.0 * (1.0 - progress).max(0.0)
        };
        let footer = Self::new(
            body,
            notch_radius(body, shape, dpi),
            window_width,
            dpi,
            top + 5.0 * (1.0 - shown),
            compact,
        );
        (footer.plate.y < body.height).then_some(footer)
    }

    fn new(
        body: NotchBody,
        radius: f32,
        window_width: i32,
        dpi: u32,
        top: f32,
        compact: bool,
    ) -> Self {
        let (inset, height, padding) = if compact {
            (10.0, COMPACT_ROW_HEIGHT, 12.0)
        } else {
            (INSET, PLATE_HEIGHT, 12.0)
        };
        let mut plate = scaled_bounds((0.0, top, 0.0, height), dpi);
        let width = if compact {
            body.width.min(scaled(COMPACT_WIDTH, dpi))
        } else {
            body.width
        };
        plate.width = width.min(window_width) - 2 * scaled(inset, dpi);
        plate.x = (body.width - plate.width).div_euclid(2);
        let right = plate.x + plate.width - scaled(padding, dpi);
        let mut button = scaled_bounds((0.0, top + (height - 24.0) / 2.0, BUTTON_WIDTH, 24.0), dpi);
        button.x = right - button.width;
        let left = plate.x + scaled(if compact { 34.0 } else { 36.0 }, dpi);
        let text = Bounds {
            x: left,
            width: right - left,
            ..plate
        };
        Self {
            plate,
            text,
            button,
            compact,
            body,
            radius,
            dpi,
        }
    }

    pub fn plate_radius(self) -> f32 {
        (if self.compact { 15.0 } else { 16.0 }) * self.dpi as f32 / 96.0
    }

    pub fn label_right(self) -> i32 {
        self.button.x - scaled(8.0, self.dpi)
    }

    pub fn hit(self, point: (i32, i32), action: bool) -> bool {
        let (x, y) = (
            (point.0 - self.body.left) as f32 + 0.5,
            point.1 as f32 + 0.5,
        );
        if y >= self.body.height as f32 {
            return false;
        }
        let edge = notch_left(self.body, self.radius, y) - self.body.left as f32;
        if x < edge || x >= self.body.width as f32 - edge {
            return false;
        }
        let bounds = if action { self.button } else { self.plate };
        let radius = if action {
            bounds.height as f32 / 2.0
        } else {
            self.plate_radius()
        };
        rounded_hit(bounds, radius, x, y)
    }
}

fn glance_shape(media: bool) -> NotchShape {
    pill(
        if media { MEDIA_NOTCH } else { NOTCH },
        NOTCH_EXPANDED,
        PILL_PERCENT.load(Relaxed),
    )
}

fn glance_row_top(media: bool) -> f32 {
    glance_shape(media).height - 2.0
}

pub fn notice_glance_at(width: i32, dpi: u32, media: bool, (x, y): (i32, i32)) -> bool {
    let shape = glance_shape(media);
    let body = notch_body(width, dpi, shape);
    if !(0..scaled(glance_row_top(media), dpi)).contains(&y) {
        return false;
    }
    let edge = notch_left(body, notch_radius(body, shape, dpi), y as f32 + 0.5);
    let x = x as f32 + 0.5;
    x >= edge && x < (2 * body.left + body.width) as f32 - edge
}

fn rounded_hit(bounds: Bounds, radius: f32, x: f32, y: f32) -> bool {
    let x = x - bounds.x as f32;
    let y = y - bounds.y as f32;
    let (width, height) = (bounds.width as f32, bounds.height as f32);
    if !(0.0..width).contains(&x) || !(0.0..height).contains(&y) {
        return false;
    }
    let radius = radius.min(width.min(height) / 2.0);
    let dx = x - x.clamp(radius, width - radius);
    let dy = y - y.clamp(radius, height - radius);
    dx * dx + dy * dy <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_snooze_uses_the_drawn_capsule_and_respects_the_body_clip() {
        for dpi in [96, 120, 144] {
            let shape = NotchShape {
                width: 296.0,
                height: 64.0,
                radius: 32.0,
                ..NOTCH
            };
            let body = notch_body(scaled(440.0, dpi), dpi, shape);
            let radius = notch_radius(body, shape, dpi);
            let row = NoticeFooter::new(body, radius, scaled(440.0, dpi), dpi, 28.0, true);
            let x = body.left + row.button.x + row.button.width / 2;
            let y = row.button.y + row.button.height / 2;
            assert!(row.hit((x, y), true));
            assert!(row.hit((body.left + row.text.x, y), false));
            assert!(!row.hit((body.left + row.text.x, y), true));
            assert!(!row.hit((body.left + row.button.x, row.button.y), true));
            let cropped = NoticeFooter::new(
                NotchBody { height: y, ..body },
                radius,
                scaled(440.0, dpi),
                dpi,
                28.0,
                true,
            );
            assert!(!cropped.hit((x, y), true));
        }
    }

    #[test]
    fn the_open_footer_clears_page_controls() {
        assert_eq!(plate_top(Tab::Media, false, 0), 120);
        assert_eq!(plate_top(Tab::Media, true, 0), 132);
        assert!(plate_top(Tab::Prayer, false, 0) >= 126);
        assert!(plate_top(Tab::Timer, false, reminders_height(3)) >= 154);
    }
}
