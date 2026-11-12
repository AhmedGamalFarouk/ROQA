use crate::{
    audio::BARS,
    dock::{self, Buttons},
    motion,
};
use std::sync::atomic::{AtomicU16, Ordering::Relaxed};

/// Notch outline in DIPs: body size, bottom corner radius and the concave "ear" fillets that
/// blend the top corners into the screen edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotchShape {
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub ears: f32,
}
pub const NOTCH: NotchShape = NotchShape {
    width: 150.0,
    height: 30.0,
    radius: 14.0,
    ears: 10.0,
};
pub const NOTCH_EXPANDED: NotchShape = NotchShape {
    width: 320.0,
    height: 80.0,
    radius: 24.0,
    ears: 16.0,
};
/// While media is playing with a title or artist the pill widens for the cover slot and title, and the panel grows
/// for the cover, title, progress, controls and volume rows (direction B, provisional;
/// docs/design/roqa-visual-spec.md).
pub const MEDIA_NOTCH: NotchShape = NotchShape {
    width: 262.0,
    ..NOTCH
};
pub const MEDIA_EXPANDED: NotchShape = NotchShape {
    width: 360.0,
    height: 120.0,
    ..NOTCH_EXPANDED
};
/// The notch HWND never moves or resizes: the media panel and ears plus room for the open
/// overshoot and the soft shadow, keeping the plain panel's 40 DIP of side and 44 DIP of bottom
/// shadow room. `notch_region` bounds body input; a separate transparent HWND draws the shadow.
pub const NOTCH_WINDOW: (i32, i32) = (440, 164);
/// Pointer slop around the drawn body that still counts as hovering it.
const NOTCH_HOVER_SLOP: f32 = 4.0;

/// The closed pill's size in percent (the Settings "Notch size"; 0 or 100 is the designed size).
static PILL_PERCENT: AtomicU16 = AtomicU16::new(100);
pub fn set_pill_percent(percent: u16) {
    PILL_PERCENT.store(percent, Relaxed);
}
/// The closed pill `from` at `percent`, never wider or taller than the open panel `to`.
fn pill(from: NotchShape, to: NotchShape, percent: u16) -> NotchShape {
    let k = f32::from(percent.max(100)) / 100.0;
    NotchShape {
        width: (from.width * k).min(to.width),
        height: (from.height * k).min(to.height),
        radius: from.radius * k,
        ears: from.ears * k,
    }
}

/// Shape at an animation progress, for the media or the plain clock notch: size follows the
/// spring's overshoot; corners and ears never overshoot.
pub fn notch_shape(progress: f32, media: bool) -> NotchShape {
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let settled = progress.clamp(0.0, 1.0);
    let (from, to) = if media {
        (MEDIA_NOTCH, MEDIA_EXPANDED)
    } else {
        (NOTCH, NOTCH_EXPANDED)
    };
    let from = pill(from, to, PILL_PERCENT.load(Relaxed));
    NotchShape {
        width: lerp(from.width, to.width, progress),
        height: lerp(from.height, to.height, progress),
        radius: lerp(from.radius, to.radius, settled),
        ears: lerp(from.ears, to.ears, settled),
    }
}

/// The notch body in window pixels. Whole pixels keep edges crisp, and drawing, hover and the
/// window region all use this one rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotchBody {
    pub left: i32,
    pub width: i32,
    pub height: i32,
    pub ears: i32,
}
pub fn notch_body(window_width: i32, dpi: u32, shape: NotchShape) -> NotchBody {
    let width = scaled(shape.width, dpi);
    NotchBody {
        left: (window_width - width).div_euclid(2),
        width,
        height: scaled(shape.height, dpi),
        ears: scaled(shape.ears, dpi),
    }
}

/// Horizontal silhouette edge at a physical Y coordinate. This uses the same circular ears and
/// bottom corners, pixel-rounded body and radius clamp as the Direct2D outline.
fn notch_left(body: NotchBody, radius: f32, y: f32) -> f32 {
    let left = body.left as f32;
    let ears = body.ears as f32;
    if y < ears {
        left - ears + (ears * ears - (y - ears).powi(2)).max(0.0).sqrt()
    } else if y > body.height as f32 - radius {
        left + radius
            - (radius * radius - (y - body.height as f32 + radius).powi(2))
                .max(0.0)
                .sqrt()
    } else {
        left
    }
}

pub fn notch_radius(body: NotchBody, shape: NotchShape, dpi: u32) -> f32 {
    (shape.radius * dpi as f32 / 96.0)
        .min((body.height - body.ears) as f32)
        .min(body.width as f32 / 2.0)
        .max(0.0)
}

/// One-pixel rows contain the anti-aliased outline, with at most a one-pixel edge fringe. They
/// exclude the shadow and empty corners; the height is bounded by the actual HWND.
pub fn notch_rows(
    window: Bounds,
    dpi: u32,
    progress: f32,
    media: bool,
    rail: Option<Rail>,
) -> Vec<Bounds> {
    let shape = notch_shape(progress, media);
    let body = notch_body(window.width, dpi, shape);
    let radius = notch_radius(body, shape, dpi);
    let mut rows: Vec<Bounds> = (0..body.height.min(window.height))
        .map(|y| {
            let edge = notch_left(body, radius, y as f32);
            let left = (edge.floor() as i32).max(0);
            let right = ((2 * body.left + body.width) as f32 - edge).ceil() as i32;
            Bounds {
                x: left,
                y,
                width: right.min(window.width) - left,
                height: 1,
            }
        })
        .collect();
    // The handle hangs from the panel's bottom edge, with its own concave ears and rounded corners.
    if let Some(rail) = rail
        && let Some(hang) = rail_bounds(window.width, dpi, progress, media, rail)
    {
        let shape = NotchBody {
            left: hang.x,
            width: hang.width,
            height: hang.height,
            ears: scaled(RAIL_EARS, dpi).min(hang.height),
        };
        let radius = rail_radius(shape, rail.p, dpi);
        rows.extend(
            (0..hang.height)
                .take_while(|y| hang.y + y < window.height)
                .map(|y| {
                    let edge = notch_left(shape, radius, y as f32);
                    let left = (edge.floor() as i32).max(0);
                    let right = ((2 * shape.left + shape.width) as f32 - edge).ceil() as i32;
                    Bounds {
                        x: left,
                        y: hang.y + y,
                        width: right.min(window.width) - left,
                        height: 1,
                    }
                }),
        );
    }
    rows
}

/// Hover follows the silhouette with four DIP of slop; transparent side bands do not hold it open.
pub fn notch_hit(
    window_width: i32,
    dpi: u32,
    progress: f32,
    media: bool,
    rail: Option<Rail>,
    x: i32,
    y: i32,
) -> bool {
    let shape = notch_shape(progress, media);
    let body = notch_body(window_width, dpi, shape);
    let slop = scaled(NOTCH_HOVER_SLOP, dpi);
    if let Some(rail) = rail
        && let Some(hang) = rail_bounds(window_width, dpi, progress, media, rail)
        && (hang.y..hang.y + hang.height + slop).contains(&y)
        && (hang.x - slop..hang.x + hang.width + slop).contains(&x)
    {
        return true;
    }
    if !(0..body.height + slop).contains(&y) {
        return false;
    }
    let radius = notch_radius(body, shape, dpi);
    let left = notch_left(body, radius, (y - slop).max(0) as f32) - slop as f32;
    let right = (2 * body.left + body.width) as f32 - left;
    x as f32 >= left && (x as f32) < right
}

/// Previous, play/pause and next in the media panel's controls row (direction B), then the volume
/// row's mute toggle (`audio::MUTE`), in DIPs from the body's left and top edges: x, y, width,
/// height.
const MEDIA_CONTROLS: [(f32, f32, f32, f32); 4] = [
    (100.0, 66.0, 28.0, 32.0),
    (148.0, 66.0, 32.0, 32.0),
    (200.0, 66.0, 28.0, 32.0),
    (100.0, 100.0, 24.0, 16.0),
];
/// The volume level bar's hit box, right of the mute toggle and ending before the settings button.
const VOLUME_BAR: (f32, f32, f32, f32) = (132.0, 100.0, 188.0, 16.0);

fn scaled_bounds((x, y, width, height): (f32, f32, f32, f32), dpi: u32) -> Bounds {
    Bounds {
        x: scaled(x, dpi),
        y: scaled(y, dpi),
        width: scaled(width, dpi),
        height: scaled(height, dpi),
    }
}

/// The media controls in pixels from the body's left and top edges. They keep their panel position
/// while the body morphs: the body clip reveals them, and they fade in with `motion::reveal`.
pub fn media_controls(dpi: u32) -> [Bounds; 4] {
    MEDIA_CONTROLS.map(|control| scaled_bounds(control, dpi))
}

/// The volume bar's hit box in pixels from the body's left and top edges, placed like the controls.
pub fn volume_bar(dpi: u32) -> Bounds {
    scaled_bounds(VOLUME_BAR, dpi)
}

/// The volume a click at a window point sets: 0 at the bar's first pixel to 1 at its last; none
/// outside the bar or while the row is not drawn.
pub fn volume_at(window_width: i32, dpi: u32, progress: f32, x: i32, y: i32) -> Option<f32> {
    let bar = volume_bar(dpi);
    panel_offset(window_width, dpi, progress, bar, (x, y))
        .map(|offset| offset as f32 / (bar.width - 1).max(1) as f32)
}

/// The volume a drag at window column `x` means: like `volume_at` but clamped to the bar's ends and ignoring the
/// row, so a captured drag keeps tracking the pointer outside the bar. None while the row is not drawn.
pub fn volume_fraction(window_width: i32, dpi: u32, progress: f32, x: i32) -> Option<f32> {
    if motion::reveal(progress) <= 0.0 {
        return None;
    }
    let bar = volume_bar(dpi);
    let body = notch_body(window_width, dpi, notch_shape(progress, true));
    let offset = (x - body.left - bar.x) as f32;
    Some((offset / (bar.width - 1).max(1) as f32).clamp(0.0, 1.0))
}

/// The settings button in the expanded panel's bottom-right corner, in DIPs from the body's left and top
/// edges (media panel, plain panel): 16 square, 12 from the right edge and 8 from the bottom.
const GEAR_SIZE: f32 = 16.0;
pub fn gear(dpi: u32, media: bool) -> Bounds {
    let shape = if media {
        MEDIA_EXPANDED
    } else {
        NOTCH_EXPANDED
    };
    scaled_bounds(
        (
            shape.width - 12.0 - GEAR_SIZE,
            shape.height - 8.0 - GEAR_SIZE,
            GEAR_SIZE,
            GEAR_SIZE,
        ),
        dpi,
    )
}

/// Whether a window point is on the settings button (a little slack around it); false while the panel is closed.
pub fn gear_at(window_width: i32, dpi: u32, progress: f32, media: bool, x: i32, y: i32) -> bool {
    if motion::reveal(progress) < 1.0 {
        return false;
    }
    let (g, slack) = (gear(dpi, media), scaled(4.0, dpi));
    let body = notch_body(window_width, dpi, notch_shape(progress, media));
    let x = x - body.left;
    (g.x - slack..g.x + g.width + slack).contains(&x)
        && (g.y - slack..g.y + g.height + slack).contains(&y)
}

/// The Limits page's Refresh button beside its title, in DIPs from the body's left and top edges.
const REFRESH: (f32, f32, f32, f32) = (80.0, 8.0, 76.0, 20.0);
pub fn refresh(dpi: u32) -> Bounds {
    scaled_bounds(REFRESH, dpi)
}

/// Whether a window point is on the Refresh button; false while the panel is closed.
pub fn refresh_at(window_width: i32, dpi: u32, progress: f32, media: bool, x: i32, y: i32) -> bool {
    if motion::reveal(progress) < 1.0 {
        return false;
    }
    let button = refresh(dpi);
    let body = notch_body(window_width, dpi, notch_shape(progress, media));
    let x = x - body.left;
    (button.x..button.x + button.width).contains(&x)
        && (button.y..button.y + button.height).contains(&y)
}

/// The visualizer slot right of the controls row (slice 7, provisional), in DIPs from the body's
/// left and top edges: `BARS` bars `METER_BAR` wide, spread across it and bottom-aligned, ending 16
/// before the body's right edge and centred on the controls row.
const METER: (f32, f32, f32, f32) = (312.0, 72.0, 32.0, 20.0);
const METER_BAR: f32 = 4.0;

/// The visualizer slot in pixels from the body's left and top edges, and each bar's width.
pub fn meter(dpi: u32) -> (Bounds, i32) {
    (scaled_bounds(METER, dpi), scaled(METER_BAR, dpi))
}

/// Each bar's height in whole pixels for its peak (0 to 1): from the bar's width, a dot at silence,
/// to the slot's height. The bars repaint only when one of these changes.
pub fn meter_heights(peaks: [f32; BARS], dpi: u32) -> [i32; BARS] {
    let (slot, bar) = meter(dpi);
    peaks.map(|peak| bar + (peak * (slot.height - bar) as f32).round() as i32)
}

/// The plain pill's minutes slot (slice S5, PROVISIONAL), in DIPs from the body's left and top edges: x, y,
/// width, height, right of the centred 15 DIP clock; the media pill is full (cover, eyes, clock) and shows none.
const TIMER_PILL: (f32, f32, f32, f32) = (108.0, 8.0, 30.0, 14.0);

/// The pages other than Media (Timer, Shelf), in DIPs from the body's left and top edges: a title row, the
/// Timer page's big value under it, and a small clock at the top right (right-aligned, 16 from the edge).
const PAGE_TITLE: (f32, f32, f32, f32) = (16.0, 12.0, 160.0, 16.0);
const PAGE_VALUE: (f32, f32, f32, f32) = (16.0, 30.0, 220.0, 32.0);
const PAGE_CLOCK: (f32, f32, f32, f32) = (0.0, 11.0, 100.0, 16.0);
const PAGE_CLOCK_MARGIN: f32 = 16.0;
/// The Shelf page's hint beside the empty shelf's add slot.
const SHELF_HINT: (f32, f32, f32, f32) = (112.0, 40.0, 192.0, 16.0);

pub fn page_title(dpi: u32) -> Bounds {
    scaled_bounds(PAGE_TITLE, dpi)
}
pub fn page_value(dpi: u32) -> Bounds {
    scaled_bounds(PAGE_VALUE, dpi)
}
pub fn shelf_hint(dpi: u32) -> Bounds {
    scaled_bounds(SHELF_HINT, dpi)
}
/// The page clock for the plain or media panel.
pub fn page_clock(media: bool, dpi: u32) -> Bounds {
    let width = if media {
        MEDIA_EXPANDED
    } else {
        NOTCH_EXPANDED
    }
    .width;
    let (_, y, w, h) = PAGE_CLOCK;
    scaled_bounds((width - PAGE_CLOCK_MARGIN - w, y, w, h), dpi)
}

/// The plain pill's minutes slot in pixels from the body's left and top edges.
pub fn timer_pill(dpi: u32) -> Bounds {
    scaled_bounds(TIMER_PILL, dpi)
}

/// The power pill's text slot (features B1, PROVISIONAL, no design review), in DIPs from the body's left and
/// top edges: x, y, width, height. Plain panel: the status row under the date (centred). Media panel: the
/// timer's pocket under the 72 DIP cover and left of the controls (from x 100), leading-aligned.
const POWER_NOTICE: [(f32, f32, f32, f32); 2] =
    [(12.0, 66.0, 296.0, 12.0), (16.0, 92.0, 84.0, 16.0)];

/// The power pill's text slot in pixels from the body's left and top edges.
pub fn power_notice(media: bool, dpi: u32) -> Bounds {
    scaled_bounds(POWER_NOTICE[usize::from(media)], dpi)
}

/// The file shelf's `shelf::VISIBLE` square slots (slice S6, PROVISIONAL, no design review), each
/// `SHELF_SLOT` DIPs with the slot origin of the panel kind's first one, in DIPs from the body's left and
/// top edges, then one slot further right per entry. On the Shelf page, under the title, on both panel kinds.
/// Each slot draws a 20 DIP system icon with the file name under it.
const SHELF_ORIGIN: [(f32, f32); 2] = [(16.0, 30.0), (16.0, 30.0)];
const SHELF_SLOT: (f32, f32) = (96.0, 36.0);

/// A shelf slot in pixels from the body's left and top edges, for the plain or media panel.
pub fn shelf_slot(media: bool, index: usize, dpi: u32) -> Bounds {
    let (x, y) = SHELF_ORIGIN[usize::from(media)];
    scaled_bounds(
        (
            x + SHELF_SLOT.0 * index as f32,
            y,
            SHELF_SLOT.0,
            SHELF_SLOT.1,
        ),
        dpi,
    )
}

/// The shelf slot under a window point at `progress`; none while the panel rows are not drawn. The caller
/// ignores slots past the entries it has.
pub fn shelf_at(
    window_width: i32,
    dpi: u32,
    progress: f32,
    media: bool,
    x: i32,
    y: i32,
) -> Option<usize> {
    if motion::reveal(progress) <= 0.0 {
        return None;
    }
    let body = notch_body(window_width, dpi, notch_shape(progress, media));
    (0..crate::shelf::VISIBLE).find(|&index| {
        let slot = shelf_slot(media, index, dpi);
        let (x, y) = (x - body.left - slot.x, y - slot.y);
        (0..slot.width).contains(&x) && (0..slot.height.min(body.height - slot.y)).contains(&y)
    })
}

/// The progress bar's target in the media panel (slice 4, provisional), in DIPs from the body's
/// left and top edges: the drawn 4 DIP bar is centred in it, between the elapsed and total times.
const MEDIA_PROGRESS: (f32, f32, f32, f32) = (140.0, 50.0, 164.0, 14.0);

/// The progress bar's target in pixels from the body's left and top edges; like the controls, it
/// keeps its panel place while the body morphs.
pub fn media_progress(dpi: u32) -> Bounds {
    scaled_bounds(MEDIA_PROGRESS, dpi)
}

/// A window point's x offset inside `target` (pixels from the body's left and top) at `progress`;
/// none while the panel rows are not drawn, or outside the part of `target` the body shows.
fn panel_offset(
    window_width: i32,
    dpi: u32,
    progress: f32,
    target: Bounds,
    (x, y): (i32, i32),
) -> Option<i32> {
    if motion::reveal(progress) <= 0.0 {
        return None;
    }
    let body = notch_body(window_width, dpi, notch_shape(progress, true));
    let x = x - body.left - target.x;
    let rows = target.y..(target.y + target.height).min(body.height);
    ((0..target.width).contains(&x) && rows.contains(&y)).then_some(x)
}

/// The media control under a window point at `progress`; none while the row is not drawn.
pub fn media_control_at(
    window_width: i32,
    dpi: u32,
    progress: f32,
    x: i32,
    y: i32,
) -> Option<usize> {
    media_controls(dpi)
        .into_iter()
        .position(|control| panel_offset(window_width, dpi, progress, control, (x, y)).is_some())
}

/// Where a window point falls along the progress bar, from 0 at its left edge towards 1.
pub fn media_seek_at(window_width: i32, dpi: u32, progress: f32, x: i32, y: i32) -> Option<f64> {
    let bar = media_progress(dpi);
    panel_offset(window_width, dpi, progress, bar, (x, y))
        .map(|offset| f64::from(offset) / f64::from(bar.width))
}

/// The tab handle (design round 2, T3 "Hanging handle"): a bar hanging off the open panel's bottom edge,
/// dots at rest that grow into 24 DIP icon buttons as `p` goes 0 to 1. The active tab is a longer bar at
/// rest. `count` tabs are shown, `active` is the position of the selected one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rail {
    pub count: usize,
    pub active: usize,
    pub p: f32,
}

pub const RAIL_EARS: f32 = 6.0;
const RAIL_PAD_X: f32 = 8.0;

/// Dot or button side, the active one's width, the gap between items and the vertical padding, in DIPs.
fn rail_metrics(p: f32) -> (f32, f32, f32, f32) {
    let lerp = |a: f32, b: f32| a + (b - a) * p.clamp(0.0, 1.0);
    (
        lerp(5.0, 24.0),
        lerp(14.0, 24.0),
        lerp(5.0, 4.0),
        lerp(3.0, 4.0),
    )
}

/// The handle's full size in DIPs: 70 x 11 at rest and 152 x 32 open for five tabs.
pub fn rail_size(rail: Rail) -> (f32, f32) {
    let (side, active, gap, pad) = rail_metrics(rail.p);
    let others = rail.count.saturating_sub(1) as f32;
    (
        2.0 * RAIL_PAD_X + active + others * (side + gap),
        2.0 * pad + side,
    )
}

/// The handle's bottom corner radius in pixels (7 DIP at rest, 14 open), kept inside its shape.
pub fn rail_radius(shape: NotchBody, p: f32, dpi: u32) -> f32 {
    scaled(7.0 + 7.0 * p.clamp(0.0, 1.0), dpi)
        .min(shape.height - shape.ears)
        .min(shape.width / 2)
        .max(0) as f32
}

/// The handle in window pixels, hanging from the body's bottom edge and growing with the panel's reveal;
/// none while the panel is not drawn or fewer than two tabs show.
pub fn rail_bounds(
    window_width: i32,
    dpi: u32,
    progress: f32,
    media: bool,
    rail: Rail,
) -> Option<Bounds> {
    let reveal = motion::reveal(progress);
    if reveal <= 0.0 || rail.count < 2 {
        return None;
    }
    let body = notch_body(window_width, dpi, notch_shape(progress, media));
    let (width, height) = rail_size(rail);
    let width = scaled(width, dpi);
    let height = (scaled(height, dpi) as f32 * reveal).round() as i32;
    (height > 0).then_some(Bounds {
        x: (window_width - width).div_euclid(2),
        y: body.height,
        width,
        height,
    })
}

/// Handle item `index` in window pixels; they sit left to right inside the handle, the active one longer.
pub fn rail_item(handle: Bounds, dpi: u32, rail: Rail, index: usize) -> Bounds {
    let (side, active, gap, pad) = rail_metrics(rail.p);
    let width = |item: usize| if item == rail.active { active } else { side };
    let before: f32 = (0..index).map(|item| width(item) + gap).sum();
    Bounds {
        x: handle.x + scaled(RAIL_PAD_X + before, dpi),
        y: handle.y + scaled(pad, dpi),
        width: scaled(width(index), dpi),
        height: scaled(side, dpi),
    }
}

/// Whether a window point is on the drawn handle (what makes it open).
pub fn rail_hit(
    window_width: i32,
    dpi: u32,
    progress: f32,
    media: bool,
    rail: Rail,
    (x, y): (i32, i32),
) -> bool {
    rail_bounds(window_width, dpi, progress, media, rail).is_some_and(|hang| {
        (hang.x..hang.x + hang.width).contains(&x) && (hang.y..hang.y + hang.height).contains(&y)
    })
}

/// The tab position under a window point. The gap beside an item belongs to it, so a click between two
/// buttons still lands; a point outside the item row hits none.
pub fn rail_tab_at(
    window_width: i32,
    dpi: u32,
    progress: f32,
    media: bool,
    rail: Rail,
    (x, y): (i32, i32),
) -> Option<usize> {
    let hang = rail_bounds(window_width, dpi, progress, media, rail)?;
    let first = rail_item(hang, dpi, rail, 0);
    if !(first.y..first.y + first.height).contains(&y) {
        return None;
    }
    let half_gap = scaled(rail_metrics(rail.p).2, dpi) / 2;
    (0..rail.count).find(|&index| {
        let item = rail_item(hang, dpi, rail, index);
        (item.x - half_gap..item.x + item.width + half_gap).contains(&x)
    })
}

fn scaled(dips: f32, dpi: u32) -> i32 {
    (dips * dpi as f32 / 96.0).round() as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceKind {
    Notch,
    Dock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// How the dock sits: floating above the bottom edge (the original look), or attached to it like the notch
/// hangs from the top: flat bottom, rounded top corners, flush with the work area's bottom edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DockStyle {
    #[default]
    Floating,
    Attached,
}
impl DockStyle {
    pub const ALL: [Self; 2] = [Self::Floating, Self::Attached];
    pub fn key(self) -> &'static str {
        ["floating", "attached"][self as usize]
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|style| style.key() == key)
    }
    pub fn label(self) -> &'static str {
        ["Floating", "Attached to the bottom edge"][self as usize]
    }
}

/// The attached dock's top corner radius in pixels: the notch's rounding, never more than the body allows.
pub fn attached_radius(bounds: Bounds, dpi: u32) -> f32 {
    (dock::px(dock::RADIUS_DIPS, dpi) as f32)
        .min(bounds.height as f32)
        .min(bounds.width as f32 / 2.0)
}

/// The floating placement, as the tests across modules use it.
#[cfg(test)]
pub fn placement(work: Bounds, dpi: u32, kind: SurfaceKind, dock_slots: usize) -> Bounds {
    placement_styled(
        work,
        dpi,
        kind,
        dock_slots,
        DockStyle::Floating,
        Buttons::NONE,
    )
}

/// `dock_slots` and `buttons` size the dock (picker slot plus app groups, taskbar buttons); the notch ignores them and `style`.
pub fn placement_styled(
    work: Bounds,
    dpi: u32,
    kind: SurfaceKind,
    dock_slots: usize,
    style: DockStyle,
    buttons: Buttons,
) -> Bounds {
    let scale = |dips: i32| dock::px(dips, dpi);
    let (width, height) = match kind {
        SurfaceKind::Notch => (scale(NOTCH_WINDOW.0), scale(NOTCH_WINDOW.1)),
        SurfaceKind::Dock => (
            dock::width_px(buttons, dock_slots, dpi),
            scale(dock::HEIGHT_DIPS),
        ),
    };
    let attached = kind == SurfaceKind::Dock && style == DockStyle::Attached;
    // Attached has no margin: it sits on the work area's bottom edge (above a bottom taskbar, at the screen
    // edge when the taskbar is elsewhere or hides).
    let margin = if attached {
        0
    } else {
        scale(12).min(work.height.max(1) / 4)
    };
    let width = width.clamp(1, work.width.max(1));
    let height = height.clamp(1, (work.height - margin * 2).max(1));
    Bounds {
        x: work.x + (work.width - width) / 2,
        y: match kind {
            // Flush with the top edge, like a hardware notch.
            SurfaceKind::Notch => work.y,
            SurfaceKind::Dock => work.y + work.height - margin - height,
        },
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_pill_grows_with_the_notch_size_and_stays_inside_the_panel() {
        // Auto (0) and 100 are the designed size, exactly.
        assert_eq!(pill(NOTCH, NOTCH_EXPANDED, 0), NOTCH);
        assert_eq!(pill(NOTCH, NOTCH_EXPANDED, 100), NOTCH);
        let big = pill(NOTCH, NOTCH_EXPANDED, 150);
        assert_eq!(
            (big.width, big.height, big.radius, big.ears),
            (225.0, 45.0, 21.0, 15.0)
        );
        // The media pill would pass its panel's width, so it stops there.
        assert_eq!(
            pill(MEDIA_NOTCH, MEDIA_EXPANDED, 150).width,
            MEDIA_EXPANDED.width
        );
    }
    #[test]
    fn buttons_widen_both_dock_styles_around_the_same_centre() {
        let work = Bounds {
            x: 100,
            y: 50,
            width: 1920,
            height: 1040,
        };
        for style in [DockStyle::Floating, DockStyle::Attached] {
            let plain = placement_styled(work, 144, SurfaceKind::Dock, 5, style, Buttons::NONE);
            for mask in 1..32u32 {
                let buttons = Buttons::of(std::array::from_fn(|i| mask >> i & 1 == 1));
                let at = placement_styled(work, 144, SurfaceKind::Dock, 5, style, buttons);
                assert_eq!(at.width, dock::width_px(buttons, 5, 144));
                assert!(at.width > plain.width, "{style:?} {mask}");
                assert_eq!((at.y, at.height), (plain.y, plain.height));
                // Centred on the work area, to the pixel.
                assert_eq!(at.x, work.x + (work.width - at.width) / 2);
            }
            // The notch ignores buttons.
            assert_eq!(
                placement_styled(
                    work,
                    144,
                    SurfaceKind::Notch,
                    0,
                    style,
                    Buttons::of([true; 5])
                ),
                placement(work, 144, SurfaceKind::Notch, 0)
            );
            // A dock wider than the work area is clamped to it.
            let wide = placement_styled(
                work,
                96,
                SurfaceKind::Dock,
                100,
                style,
                Buttons::of([true; 5]),
            );
            assert_eq!(wide.width, work.width);
        }
    }

    #[test]
    fn the_attached_dock_sits_flush_on_the_bottom_edge_and_floating_does_not_move() {
        let work = Bounds {
            x: -1920,
            y: -200,
            width: 1920,
            height: 1040,
        };
        let floating = placement(work, 144, SurfaceKind::Dock, 5);
        let styled =
            |style| placement_styled(work, 144, SurfaceKind::Dock, 5, style, Buttons::NONE);
        assert_eq!(styled(DockStyle::Floating), floating);
        let attached = styled(DockStyle::Attached);
        assert_eq!(
            (attached.x, attached.width, attached.height),
            (floating.x, floating.width, floating.height)
        );
        assert_eq!(attached.y + attached.height, work.y + work.height);
        assert!(floating.y + floating.height < work.y + work.height);
        // The notch ignores the style.
        assert_eq!(
            placement_styled(
                work,
                144,
                SurfaceKind::Notch,
                0,
                DockStyle::Attached,
                Buttons::NONE
            ),
            placement(work, 144, SurfaceKind::Notch, 0)
        );
    }

    #[test]
    fn the_attached_dock_stays_inside_tiny_work_areas_and_rounds_within_its_body() {
        for dpi in [0, 96, 144, 384] {
            let work = Bounds {
                x: 10,
                y: 20,
                width: 30,
                height: 10,
            };
            let at = placement_styled(
                work,
                dpi,
                SurfaceKind::Dock,
                40,
                DockStyle::Attached,
                Buttons::NONE,
            );
            assert!(at.x >= 10 && at.y >= 20);
            assert!(at.x + at.width <= 40 && at.y + at.height <= 30);
            let radius = attached_radius(at, dpi);
            assert!(radius <= at.height as f32 && radius <= at.width as f32 / 2.0);
        }
        let at = placement_styled(
            Bounds {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
            },
            96,
            SurfaceKind::Dock,
            5,
            DockStyle::Attached,
            Buttons::NONE,
        );
        assert_eq!(attached_radius(at, 96), 20.0);
    }

    #[test]
    fn dock_style_keys_round_trip() {
        for style in DockStyle::ALL {
            assert_eq!(DockStyle::from_key(style.key()), Some(style));
        }
        assert_eq!(DockStyle::from_key("x"), None);
    }

    #[test]
    fn centers_with_negative_coordinates_and_reserved_taskbar_space() {
        let work = Bounds {
            x: -1920,
            y: -200,
            width: 1920,
            height: 1040,
        };
        assert_eq!(
            placement(work, 144, SurfaceKind::Dock, 5),
            Bounds {
                x: -1182,
                y: 726,
                width: 444,
                height: 96
            }
        );
    }
    #[test]
    fn scales_and_clamps_to_tiny_work_areas() {
        for dpi in [0, 96, 120, 144, 192, 384] {
            for kind in [SurfaceKind::Notch, SurfaceKind::Dock] {
                let bounds = placement(
                    Bounds {
                        x: 10,
                        y: 20,
                        width: 30,
                        height: 10,
                    },
                    dpi,
                    kind,
                    40,
                );
                assert!(bounds.x >= 10 && bounds.y >= 20);
                assert!(bounds.x + bounds.width <= 40);
                assert!(bounds.y + bounds.height <= 30);
            }
        }
    }
    #[test]
    fn primary_notch_uses_logical_dimensions_at_200_percent() {
        assert_eq!(
            placement(
                Bounds {
                    x: 0,
                    y: 0,
                    width: 3840,
                    height: 2000
                },
                192,
                SurfaceKind::Notch,
                0
            ),
            Bounds {
                x: 1480,
                y: 0,
                width: 880,
                height: 328
            }
        );
    }

    #[test]
    fn media_notch_fits_its_window_at_100_150_and_200_percent() {
        let work = Bounds {
            x: 0,
            y: 0,
            width: 3840,
            height: 2160,
        };
        // (dpi, window, media pill, media panel) in physical pixels.
        for (dpi, window, pill, panel) in [
            (96, (440, 164), (262, 30), (360, 120)),
            (144, (660, 246), (393, 45), (540, 180)),
            (192, (880, 328), (524, 60), (720, 240)),
        ] {
            let bounds = placement(work, dpi, SurfaceKind::Notch, 0);
            assert_eq!((bounds.width, bounds.height), window);
            let size = |progress| {
                let body = notch_body(bounds.width, dpi, notch_shape(progress, true));
                (body.width, body.height)
            };
            assert_eq!(size(0.0), pill);
            assert_eq!(size(1.0), panel);
            // Undershoot, hover swell, open overshoot: body and ears stay inside the HWND.
            for progress in [-0.09, 0.0, 0.07, 1.0, 1.07] {
                for media in [false, true] {
                    let body = notch_body(bounds.width, dpi, notch_shape(progress, media));
                    assert!(body.left - body.ears >= 0, "{dpi} {progress} {media}");
                    assert!(body.left + body.width + body.ears <= bounds.width);
                    assert!(body.height <= bounds.height);
                }
            }
        }
    }

    #[test]
    fn plain_notch_keeps_its_accepted_shape_and_screen_position() {
        assert_eq!(notch_shape(0.0, false), NOTCH);
        assert_eq!(notch_shape(1.0, false), NOTCH_EXPANDED);
        let work = Bounds {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1040,
        };
        for dpi in [96, 144, 192] {
            let bounds = placement(work, dpi, SurfaceKind::Notch, 0);
            // Where the accepted 400 DIP window put the body on screen.
            let old_width = dock::px(400, dpi);
            let old_x = work.x + (work.width - old_width) / 2;
            for progress in [0.0, 1.0] {
                let shape = notch_shape(progress, false);
                let body = notch_body(bounds.width, dpi, shape);
                let old = notch_body(old_width, dpi, shape);
                assert_eq!(bounds.x + body.left, old_x + old.left);
                assert_eq!((body.width, body.height), (old.width, old.height));
            }
        }
    }

    #[test]
    fn media_controls_hit_only_their_drawn_rows_once_revealed() {
        // 360 x 120 panel centred in the 440 DIP window: body x 40..400.
        let at = |progress, x, y| media_control_at(440, 96, progress, x, y);
        assert_eq!(at(1.0, 140, 66), Some(0));
        assert_eq!(at(1.0, 167, 97), Some(0));
        assert_eq!(at(1.0, 168, 80), None); // The gap between previous and play/pause.
        assert_eq!(at(1.0, 188, 80), Some(1));
        assert_eq!(at(1.0, 219, 80), Some(1));
        assert_eq!(at(1.0, 240, 80), Some(2));
        assert_eq!(at(1.0, 268, 80), None);
        assert_eq!(at(1.0, 240, 65), None);
        assert_eq!(at(1.0, 240, 98), None);
        // Collapsed, swelling or barely open: the row is not drawn, so nothing is hit.
        for progress in [0.0, 0.07, 0.35] {
            assert_eq!(at(progress, 188, 15), None);
        }
        // Opening: only the part of a chip the body already shows is a target.
        let body = notch_body(440, 96, notch_shape(0.6, true));
        assert!(body.height < 98);
        let left = body.left + 148;
        assert_eq!(at(0.6, left + 1, body.height - 1), Some(1));
        assert_eq!(at(0.6, left + 1, body.height), None);
        // Scaled with the notch DPI, which includes the text-size factor.
        for dpi in [144, 192, 240] {
            let window = scaled(440.0, dpi);
            let body = notch_body(window, dpi, notch_shape(1.0, true));
            let play = media_controls(dpi)[1];
            let (x, y) = (body.left + play.x, play.y);
            assert_eq!(media_control_at(window, dpi, 1.0, x, y), Some(1));
            let (right, bottom) = (x + play.width, y + play.height);
            assert_eq!(media_control_at(window, dpi, 1.0, right, y), None);
            assert_eq!(media_control_at(window, dpi, 1.0, x, bottom), None);
            assert!(
                bottom <= body.height
                    && body.left + media_controls(dpi)[2].x < body.left + body.width
            );
        }
    }

    #[test]
    fn the_progress_bar_maps_clicks_to_its_fraction_beside_the_controls() {
        // Body x 40..400 at 100 %: bar from 180 to 344, rows 50..64; controls from 66.
        let seek = |progress, x, y| media_seek_at(440, 96, progress, x, y);
        assert_eq!(seek(1.0, 180, 57), Some(0.0));
        assert_eq!(seek(1.0, 221, 50), Some(0.25));
        assert_eq!(seek(1.0, 343, 63), Some(163.0 / 164.0));
        for (x, y) in [(179, 57), (344, 57), (221, 49), (221, 64), (188, 80)] {
            assert_eq!(seek(1.0, x, y), None, "{x} {y}");
        }
        // No point is both a control and the bar, at any scale.
        for dpi in [96, 144, 192] {
            let window = scaled(440.0, dpi);
            let bar = media_progress(dpi);
            let controls = media_controls(dpi);
            assert!(controls.iter().all(|c| c.y >= bar.y + bar.height), "{dpi}");
            let body = notch_body(window, dpi, notch_shape(1.0, true));
            for y in 0..body.height {
                for x in (body.left..body.left + body.width).step_by(3) {
                    let both = media_seek_at(window, dpi, 1.0, x, y).is_some()
                        && media_control_at(window, dpi, 1.0, x, y).is_some();
                    assert!(!both, "{dpi} {x} {y}");
                }
            }
            assert!(body.left + bar.x + bar.width <= body.left + body.width);
        }
        // Collapsed or barely open: the row is not drawn. Opening: only the shown part is a target.
        for progress in [0.0, 0.07, 0.35] {
            assert_eq!(seek(progress, 221, 57), None);
        }
        let body = notch_body(440, 96, notch_shape(0.36, true));
        assert!(body.height > 50 && body.height < 64);
        let left = body.left + 140;
        assert_eq!(seek(0.36, left, body.height - 1), Some(0.0));
        assert_eq!(seek(0.36, left, body.height), None);
    }

    #[test]
    fn a_drag_clamps_to_the_bar_and_the_gear_sits_clear_of_every_control() {
        // Body x 40..400: the bar spans 172..360. Left of it is silent, right of it full, whatever the row.
        let drag = |progress, x| volume_fraction(440, 96, progress, x);
        assert_eq!(drag(1.0, 0), Some(0.0));
        assert_eq!(drag(1.0, 172), Some(0.0));
        assert_eq!(drag(1.0, 359), Some(1.0));
        assert_eq!(drag(1.0, 435), Some(1.0));
        assert_eq!(drag(0.0, 200), None, "closed");
        let middle = drag(1.0, 265).unwrap();
        assert!((middle - 93.0 / 187.0).abs() < 1e-6);
        // The gear is inside both panels, off the volume bar and the mute toggle, and clickable only open.
        for dpi in [96, 144, 192] {
            let window = scaled(440.0, dpi);
            for media in [true, false] {
                let shape = if media {
                    MEDIA_EXPANDED
                } else {
                    NOTCH_EXPANDED
                };
                let g = gear(dpi, media);
                assert!(
                    g.x + g.width <= scaled(shape.width, dpi)
                        && g.y + g.height <= scaled(shape.height, dpi)
                );
                let body = notch_body(window, dpi, notch_shape(1.0, media));
                let (x, y) = (body.left + g.x + g.width / 2, g.y + g.height / 2);
                assert!(gear_at(window, dpi, 1.0, media, x, y));
                assert!(!gear_at(window, dpi, 0.5, media, x, y), "closed");
                assert!(!gear_at(window, dpi, 1.0, media, x - scaled(40.0, dpi), y));
                if media {
                    let bar = volume_bar(dpi);
                    assert!(bar.x + bar.width + scaled(4.0, dpi) <= g.x, "{dpi}");
                    assert!(media_controls(dpi).iter().all(|c| c.x + c.width <= g.x));
                }
            }
        }
    }

    #[test]
    fn volume_bar_maps_its_first_and_last_pixel_to_silent_and_full() {
        // Body x 40..400: the bar spans 172..360 and 100..116 at 100 %, below the mute toggle's row.
        let at = |progress, x, y| volume_at(440, 96, progress, x, y);
        assert_eq!(at(1.0, 172, 100), Some(0.0));
        assert_eq!(at(1.0, 359, 115), Some(1.0));
        assert_eq!(at(1.0, 172 + 187 / 2, 108), Some(93.0 / 187.0));
        for (x, y) in [(171, 108), (360, 108), (200, 99), (200, 116)] {
            assert_eq!(at(1.0, x, y), None, "{x}, {y}");
        }
        assert_eq!(media_control_at(440, 96, 1.0, 140, 100), Some(3));
        assert_eq!(media_control_at(440, 96, 1.0, 172, 108), None);
        // Collapsed or barely open, the row is not drawn.
        for progress in [0.0, 0.07, 0.35] {
            assert_eq!(at(progress, 200, 108), None);
        }
        for dpi in [144, 192, 240] {
            let window = scaled(440.0, dpi);
            let body = notch_body(window, dpi, notch_shape(1.0, true));
            let bar = volume_bar(dpi);
            let (left, right) = (body.left + bar.x, body.left + bar.x + bar.width - 1);
            assert_eq!(volume_at(window, dpi, 1.0, left, bar.y), Some(0.0));
            assert_eq!(volume_at(window, dpi, 1.0, right, bar.y), Some(1.0));
            assert!(right < body.left + body.width && bar.y + bar.height <= body.height);
        }
        // Controls, the progress bar and the volume bar never share a point, so hit order is moot.
        for dpi in [96, 144, 192] {
            let window = scaled(440.0, dpi);
            let body = notch_body(window, dpi, notch_shape(1.0, true));
            for y in 0..body.height {
                for x in body.left..body.left + body.width {
                    let hits = [
                        media_control_at(window, dpi, 1.0, x, y).is_some(),
                        media_seek_at(window, dpi, 1.0, x, y).is_some(),
                        volume_at(window, dpi, 1.0, x, y).is_some(),
                    ];
                    assert!(
                        hits.iter().filter(|&&hit| hit).count() <= 1,
                        "{dpi} {x} {y}"
                    );
                }
            }
        }
    }

    #[test]
    fn silhouette_excludes_side_shadow_and_corners_during_motion() {
        for (dpi, media) in [96, 120, 192, 384]
            .into_iter()
            .flat_map(|dpi| [(dpi, false), (dpi, true)])
        {
            let window = Bounds {
                x: 0,
                y: 0,
                width: scaled(440.0, dpi),
                height: scaled(164.0, dpi),
            };
            for progress in [-0.09, 0.0, 0.07, 0.3, 0.7, 1.0, 1.07] {
                let shape = notch_shape(progress, media);
                let body = notch_body(window.width, dpi, shape);
                let rows = notch_rows(window, dpi, progress, media, None);
                let contains = |x, y| {
                    rows.iter()
                        .any(|r| r.y == y && x >= r.x && x < r.x + r.width)
                };
                let middle = window.width / 2;
                assert!(contains(middle, body.height / 2));
                assert!(notch_hit(
                    window.width,
                    dpi,
                    progress,
                    media,
                    None,
                    middle,
                    body.height / 2
                ));
                let side_x = body.left - scaled(8.0, dpi);
                let side_y = body.ears + scaled(8.0, dpi);
                assert!(!contains(side_x, side_y));
                assert!(!notch_hit(
                    window.width,
                    dpi,
                    progress,
                    media,
                    None,
                    side_x,
                    side_y
                ));
                assert!(!contains(body.left, body.height - 1));
                assert!(!notch_hit(
                    window.width,
                    dpi,
                    progress,
                    media,
                    None,
                    body.left - scaled(5.0, dpi),
                    body.height - 1
                ));
                assert!(!contains(middle, body.height + scaled(10.0, dpi)));
            }
        }
        // Independent review's concrete regression point, 10 px beside the plain panel, moved with
        // the window's 20 px wider sides.
        assert!(!notch_hit(440, 96, 1.0, false, None, 50, 40));
        assert!(
            !notch_rows(
                Bounds {
                    x: 0,
                    y: 0,
                    width: 440,
                    height: 164
                },
                96,
                1.0,
                false,
                None
            )
            .iter()
            .any(|r| r.y == 40 && r.x <= 50)
        );
    }

    #[test]
    fn meter_bars_quantize_to_whole_pixels_and_fit_beside_the_controls() {
        // A dot at silence, the slot's height at full scale, whole pixels between.
        assert_eq!(
            meter_heights([0.0, 1.0, 0.5, 0.51, 0.53], 96),
            [4, 20, 12, 12, 12]
        );
        assert_eq!(
            meter_heights([0.0, 1.0, 0.5, 0.51, 0.53], 192),
            [8, 40, 24, 24, 25]
        );
        // A steady or sub-pixel level quantizes the same, so it repaints nothing.
        assert_eq!(
            meter_heights([0.30; BARS], 96),
            meter_heights([0.33; BARS], 96)
        );
        for dpi in [96, 144, 192, 240] {
            let (slot, bar) = meter(dpi);
            let body = notch_body(scaled(440.0, dpi), dpi, notch_shape(1.0, true));
            assert!(slot.x + slot.width <= body.width && slot.y + slot.height <= body.height);
            assert!(bar * BARS as i32 <= slot.width);
            let overlaps = |other: Bounds| {
                slot.x < other.x + other.width
                    && other.x < slot.x + slot.width
                    && slot.y < other.y + other.height
                    && other.y < slot.y + slot.height
            };
            let mut targets = media_controls(dpi)
                .into_iter()
                .chain([media_progress(dpi), volume_bar(dpi)]);
            assert!(!targets.any(overlaps), "{dpi}");
        }
    }

    fn hit(a: Bounds, b: Bounds) -> bool {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    }

    #[test]
    fn page_slots_stay_inside_their_body_and_overlap_nothing_on_the_page() {
        for dpi in [96, 144, 192, 240] {
            for media in [false, true] {
                let shape = if media {
                    MEDIA_EXPANDED
                } else {
                    NOTCH_EXPANDED
                };
                let body = notch_body(scaled(440.0, dpi), dpi, shape);
                let fits = |slot: Bounds| {
                    slot.x >= 0
                        && slot.y >= 0
                        && slot.x + slot.width <= body.width
                        && slot.y + slot.height <= body.height
                };
                let (title, value, clock) =
                    (page_title(dpi), page_value(dpi), page_clock(media, dpi));
                let notice = power_notice(media, dpi);
                let shelf = shelf_slot(media, 0, dpi);
                for slot in [title, value, clock, notice, shelf, shelf_hint(dpi)] {
                    assert!(fits(slot), "{dpi} {media}");
                }
                // Timer page: title, value, clock and the notice row never share a pixel.
                let timer_page = [title, value, clock, notice];
                for (a, first) in timer_page.iter().enumerate() {
                    for second in &timer_page[a + 1..] {
                        assert!(!hit(*first, *second), "{dpi} {media}");
                    }
                }
                // Shelf page: the add slot, its hint, the title and the clock.
                let shelf_page = [title, clock, shelf, shelf_hint(dpi)];
                for (a, first) in shelf_page.iter().enumerate() {
                    for second in &shelf_page[a + 1..] {
                        assert!(!hit(*first, *second), "{dpi} {media}");
                    }
                }
            }
            // Plain pill: right of the centred clock (about x 47-103 of 150).
            let pill = timer_pill(dpi);
            let body = notch_body(scaled(440.0, dpi), dpi, NOTCH);
            assert!(
                pill.x + pill.width <= body.width && pill.x >= scaled(105.0, dpi),
                "{dpi}"
            );
        }
    }

    #[test]
    fn the_power_notice_fits_its_panel_and_overlaps_no_media_element() {
        let hit = |a: Bounds, b: Bounds| {
            a.x < b.x + b.width
                && b.x < a.x + a.width
                && a.y < b.y + b.height
                && b.y < a.y + a.height
        };
        for dpi in [96, 144, 192, 240] {
            let fits = |slot: Bounds, shape: NotchShape| {
                slot.x >= 0
                    && slot.y >= 0
                    && slot.x + slot.width <= scaled(shape.width, dpi)
                    && slot.y + slot.height <= scaled(shape.height, dpi)
            };
            assert!(fits(power_notice(false, dpi), NOTCH_EXPANDED), "{dpi}");
            let media = power_notice(true, dpi);
            assert!(fits(media, MEDIA_EXPANDED), "{dpi}");
            let cover = scaled_bounds((16.0, 14.0, 72.0, 72.0), dpi);
            let others = media_controls(dpi).into_iter().chain([
                cover,
                media_progress(dpi),
                volume_bar(dpi),
                meter(dpi).0,
            ]);
            assert!(!others.into_iter().any(|other| hit(media, other)), "{dpi}");
        }
    }

    #[test]
    fn shelf_slots_stay_inside_their_body_overlap_no_other_element_and_hit_one_target() {
        let hit = |a: Bounds, b: Bounds| {
            a.x < b.x + b.width
                && b.x < a.x + a.width
                && a.y < b.y + b.height
                && b.y < a.y + a.height
        };
        for dpi in [96, 144, 192, 240] {
            let width = scaled(440.0, dpi);
            for media in [false, true] {
                let shape = if media {
                    MEDIA_EXPANDED
                } else {
                    NOTCH_EXPANDED
                };
                let body = notch_body(width, dpi, shape);
                let slots: Vec<Bounds> = (0..crate::shelf::VISIBLE)
                    .map(|index| shelf_slot(media, index, dpi))
                    .collect();
                for slot in &slots {
                    assert!(slot.x >= 0 && slot.y >= 0, "{dpi} {media}");
                    // Whole slot and a 12 DIP right margin inside the body.
                    assert!(
                        slot.x + slot.width <= body.width - scaled(12.0, dpi),
                        "{dpi} {media}"
                    );
                    assert!(slot.y + slot.height <= body.height, "{dpi} {media}");
                }
                // The page's own title, clock and notice row; the Media page's elements are not drawn here.
                let others = [
                    page_title(dpi),
                    page_clock(media, dpi),
                    power_notice(media, dpi),
                ];
                for slot in &slots {
                    assert!(
                        !others.iter().any(|&other| hit(*slot, other)),
                        "{dpi} {media}"
                    );
                }
                // A point in a slot's middle hits that slot only: no media control, seek or volume
                // target shares it, and no pixel of one slot belongs to another.
                for (index, slot) in slots.iter().enumerate() {
                    let (x, y) = (
                        body.left + slot.x + slot.width / 2,
                        slot.y + slot.height / 2,
                    );
                    assert_eq!(shelf_at(width, dpi, 1.0, media, x, y), Some(index));
                    assert_eq!(media_control_at(width, dpi, 1.0, x, y), None);
                    assert_eq!(media_seek_at(width, dpi, 1.0, x, y), None);
                    assert_eq!(volume_at(width, dpi, 1.0, x, y), None);
                    // Just outside either edge, and above or below.
                    assert_ne!(
                        shelf_at(width, dpi, 1.0, media, body.left + slot.x - 1, y),
                        Some(index)
                    );
                    assert_eq!(shelf_at(width, dpi, 1.0, media, x, slot.y - 1), None);
                    assert_eq!(
                        shelf_at(width, dpi, 1.0, media, x, slot.y + slot.height),
                        None
                    );
                    // Not drawn while the panel is closed.
                    assert_eq!(shelf_at(width, dpi, 0.0, media, x, y), None);
                }
            }
        }
    }

    fn rail(count: usize, active: usize, p: f32) -> Rail {
        Rail { count, active, p }
    }

    #[test]
    fn the_handle_matches_the_design_sizes_and_grows_from_dots_to_buttons() {
        // Design T3: 70 x 11 at rest and 152 x 32 open for five tabs; fewer tabs are narrower.
        let (rest_w, rest_h) = rail_size(rail(5, 0, 0.0));
        assert_eq!((rest_w, rest_h), (70.0, 11.0));
        let (open_w, open_h) = rail_size(rail(5, 0, 1.0));
        assert_eq!((open_w, open_h), (152.0, 32.0));
        assert!(rail_size(rail(3, 0, 1.0)).0 < open_w);
        let mut last = rail_size(rail(3, 1, 0.0));
        for step in 1..=10 {
            let now = rail_size(rail(3, 1, step as f32 / 10.0));
            assert!(now.0 >= last.0 && now.1 >= last.1, "{step}");
            last = now;
        }
    }

    #[test]
    fn the_handle_hangs_from_the_open_panel_and_stays_inside_the_window() {
        for dpi in [96, 144, 192, 240] {
            let window = Bounds {
                x: 0,
                y: 0,
                width: scaled(440.0, dpi),
                height: scaled(164.0, dpi),
            };
            for media in [false, true] {
                let r = rail(5, 2, 1.0);
                // Collapsed or barely open: no handle.
                for progress in [-0.09, 0.0, 0.07, 0.35] {
                    assert_eq!(rail_bounds(window.width, dpi, progress, media, r), None);
                }
                for progress in [0.8, 1.0, 1.07] {
                    let body = notch_body(window.width, dpi, notch_shape(progress, media));
                    let hang = rail_bounds(window.width, dpi, progress, media, r).unwrap();
                    assert_eq!(hang.y, body.height, "joined to the panel's bottom edge");
                    assert!(hang.x >= 0 && hang.x + hang.width <= window.width);
                    assert!(
                        hang.y + hang.height <= window.height,
                        "{dpi} {media} {progress}"
                    );
                    // Centred like the body.
                    assert_eq!(hang.x + hang.width / 2, window.width / 2);
                    // The region takes its rows, and only those rows hold the pointer.
                    let rows = notch_rows(window, dpi, progress, media, Some(r));
                    let below = rows.iter().filter(|row| row.y >= body.height).count() as i32;
                    assert_eq!(below, hang.height.min(window.height - hang.y));
                    assert!(notch_hit(
                        window.width,
                        dpi,
                        progress,
                        media,
                        Some(r),
                        window.width / 2,
                        hang.y + hang.height / 2
                    ));
                    assert!(!notch_hit(
                        window.width,
                        dpi,
                        progress,
                        media,
                        None,
                        window.width / 2,
                        hang.y + hang.height / 2
                    ));
                    // Beside the handle, under the panel, is transparent again.
                    assert!(!notch_hit(
                        window.width,
                        dpi,
                        progress,
                        media,
                        Some(r),
                        hang.x - scaled(30.0, dpi),
                        hang.y + scaled(10.0, dpi)
                    ));
                }
                // It grows out of the panel with the reveal.
                let part = rail_bounds(window.width, dpi, 0.5, media, r).unwrap();
                let full = rail_bounds(window.width, dpi, 1.0, media, r).unwrap();
                assert!(part.height < full.height);
            }
        }
    }

    #[test]
    fn a_single_tab_has_no_handle() {
        let window = 440;
        assert_eq!(rail_bounds(window, 96, 1.0, true, rail(1, 0, 1.0)), None);
        assert_eq!(rail_bounds(window, 96, 1.0, true, rail(0, 0, 1.0)), None);
        let rows = notch_rows(
            Bounds {
                x: 0,
                y: 0,
                width: window,
                height: 164,
            },
            96,
            1.0,
            true,
            Some(rail(1, 0, 1.0)),
        );
        assert_eq!(rows.len(), 120, "just the panel");
    }

    #[test]
    fn handle_items_lie_inside_it_in_order_and_each_point_picks_one_tab() {
        for dpi in [96, 144, 192] {
            let width = scaled(440.0, dpi);
            for p in [0.0, 0.5, 1.0] {
                let r = rail(5, 1, p);
                let hang = rail_bounds(width, dpi, 1.0, true, r).unwrap();
                let items: Vec<Bounds> = (0..5).map(|i| rail_item(hang, dpi, r, i)).collect();
                for (i, item) in items.iter().enumerate() {
                    assert!(
                        item.x >= hang.x && item.x + item.width <= hang.x + hang.width,
                        "{dpi} {p} {i}"
                    );
                    assert!(
                        item.y >= hang.y && item.y + item.height <= hang.y + hang.height,
                        "{dpi} {p} {i}"
                    );
                    assert!(
                        i == 0 || item.x >= items[i - 1].x + items[i - 1].width,
                        "ordered, no overlap"
                    );
                    let middle = (item.x + item.width / 2, item.y + item.height / 2);
                    assert_eq!(
                        rail_tab_at(width, dpi, 1.0, true, r, middle),
                        Some(i),
                        "{dpi} {p}"
                    );
                    assert!(rail_hit(width, dpi, 1.0, true, r, middle));
                }
                // The active tab is the longer bar at rest, and equal to the others when open.
                assert!(
                    items[1].width > items[0].width || p == 1.0 && items[1].width == items[0].width,
                    "{p}"
                );
                // Above the item row, or outside the handle, picks nothing.
                assert_eq!(
                    rail_tab_at(width, dpi, 1.0, true, r, (items[2].x, hang.y - 1)),
                    None
                );
                assert_eq!(
                    rail_tab_at(width, dpi, 1.0, true, r, (hang.x - 1, items[0].y)),
                    None
                );
                assert!(!rail_hit(
                    width,
                    dpi,
                    1.0,
                    true,
                    r,
                    (hang.x - 1, items[0].y)
                ));
            }
        }
    }
}
