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
    // Capsule: radius = height / 2 (brand grammar).
    radius: 15.0,
    ears: 10.0,
};
pub const NOTCH_EXPANDED: NotchShape = NotchShape {
    width: 360.0,
    height: 128.0,
    radius: 28.0,
    ears: 14.0,
};
/// While media is playing with a title or artist the pill widens for the cover slot and title, and the panel grows
/// for the cover, title, progress, controls and volume rows (direction B, provisional;
/// docs/design/roqa-visual-spec.md).
pub const MEDIA_NOTCH: NotchShape = NotchShape {
    width: 262.0,
    ..NOTCH
};
pub const MEDIA_EXPANDED: NotchShape = NOTCH_EXPANDED;

/// The one grid every tab page sits on, in DIPs from the body's left and top edges. Pages differ in content, never
/// in padding: a 16 side margin, a header row (title left, clock right, one action just before the clock), the
/// content from `BODY_Y` on, an 8 gutter between elements, 24 high chips and 14 radius tiles (the panel's 28
/// radius less a 14 inset). The footer line and the settings button share the bottom row.
pub const PAD: f32 = 16.0;
pub const HEAD_Y: f32 = 10.0;
pub const HEAD_H: f32 = 18.0;
pub const BODY_Y: f32 = 38.0;
pub const GUTTER: f32 = 8.0;
pub const CHIP_H: f32 = 24.0;
pub const TILE_RADIUS: f32 = 14.0;
/// The 420 wide page (Widgets).
pub const WIDE_W: f32 = 420.0;
/// The notch HWND never moves or resizes: the media panel and ears plus room for the open
/// overshoot and the soft shadow, keeping the plain panel's 40 DIP of side and 44 DIP of bottom
/// shadow room (the window is 208 DIP tall so the Limits page, its notice row and the rail below fit). `notch_region` bounds body input; a separate transparent HWND draws the shadow.
pub const NOTCH_WINDOW: (i32, i32) = (440, 216);
/// Pointer slop around the drawn body that still counts as hovering it.
const NOTCH_HOVER_SLOP: f32 = 4.0;

/// A page that needs a taller panel than the plain 80 DIP (Limits): its height in DIPs, 0 for none. Set by
/// the host before each paint, like the pill size, so drawing and hit tests agree.
static PAGE_HEIGHT: AtomicU16 = AtomicU16::new(0);
pub fn set_page_height(dips: u16) {
    PAGE_HEIGHT.store(dips, Relaxed);
}
/// The Limits page's tool rows: the first at y 38 DIPs, `LIMITS_ROW` apart (two lines each: the used values, then
/// the reset times and the freshness).
pub const LIMITS_TOP: f32 = 44.0;
pub const LIMITS_ROW: f32 = 30.0;
/// The Limits page's panel height for `rows` tool rows: the rows and room for the gear under the last.
pub fn limits_height(rows: usize) -> u16 {
    (LIMITS_TOP + LIMITS_ROW * rows as f32) as u16 + 22
}
/// While a notice shows the plain panel grows by `NOTICE_ROOM` DIPs for its own row; `shown` (0 to 1) follows
/// the notice's entry tween, in permille. Set by the host before each paint, like the page height.
pub const NOTICE_ROOM: f32 = 8.0;
static NOTICE_SHOWN: AtomicU16 = AtomicU16::new(0);
pub fn set_notice_shown(shown: f32) {
    NOTICE_SHOWN.store((shown.clamp(0.0, 1.0) * 1000.0).round() as u16, Relaxed);
}
/// `height` grown for a notice `shown` (0 to 1) of the way in.
fn with_notice(height: f32, shown: f32) -> f32 {
    height + NOTICE_ROOM * shown.clamp(0.0, 1.0)
}
/// `shape` with at least the page's height, and the media panel's width for such a page (its columns need it).
/// The plain panel (`notice_row`) and a tall page also grow while a notice shows, for its own row under the
/// page. The media panel reserves a full-width row below its playback controls.
fn tall(shape: NotchShape, notice_row: bool) -> NotchShape {
    let page = f32::from(PAGE_HEIGHT.load(Relaxed));
    let shown = f32::from(NOTICE_SHOWN.load(Relaxed)) / 1000.0;
    NotchShape {
        height: if notice_row || page > 0.0 {
            with_notice(shape.height.max(page), shown)
        } else {
            shape.height + MEDIA_NOTICE_ROOM * shown
        },
        width: if page == 140.0 {
            WIDE_W
        } else if page > 0.0 {
            MEDIA_EXPANDED.width
        } else {
            shape.width
        },
        ears: if page == 140.0 { 10.0 } else { shape.ears },
        radius: if page == 140.0 { 32.0 } else { shape.radius },
    }
}

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
        (MEDIA_NOTCH, tall(MEDIA_EXPANDED, false))
    } else {
        (NOTCH, tall(NOTCH_EXPANDED, true))
    };
    let from = pill(from, to, PILL_PERCENT.load(Relaxed));
    NotchShape {
        width: lerp(from.width, to.width, progress).min(if PAGE_HEIGHT.load(Relaxed) == 140 {
            WIDE_W
        } else {
            f32::MAX
        }),
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
        let radius = rail_radius(shape, dpi);
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
/// The transport sits in one 28 high row under the progress row: previous and next as capsules, play/pause a wider
/// accent one, then the mute toggle.
pub const MEDIA_COLUMN: f32 = 108.0;
const MEDIA_CONTROLS: [(f32, f32, f32, f32); 4] = [
    (MEDIA_COLUMN, 84.0, 32.0, 28.0),
    (146.0, 84.0, 48.0, 28.0),
    (200.0, 84.0, 32.0, 28.0),
    (244.0, 88.0, 24.0, 20.0),
];
/// The volume level bar's hit box, right of the mute toggle and ending before the settings button.
const VOLUME_BAR: (f32, f32, f32, f32) = (272.0, 88.0, 46.0, 20.0);

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
/// edges (media panel, plain panel): 16 square, 16 from the right edge and 10 from the bottom, so its disc
/// clears the glass backing's rounded corner (`BACKING_INSET`) by 6.
const GEAR_SIZE: f32 = 16.0;
pub fn gear(dpi: u32, media: bool) -> Bounds {
    let shape = tall(
        if media {
            MEDIA_EXPANDED
        } else {
            NOTCH_EXPANDED
        },
        !media,
    );
    scaled_bounds(
        (
            shape.width - 16.0 - GEAR_SIZE,
            shape.height - 10.0 - GEAR_SIZE,
            GEAR_SIZE,
            GEAR_SIZE,
        ),
        dpi,
    )
}

/// The glass themes' content backing runs this far inside the open body's edge (DIPs), so the unoccupied outer
/// border is all that stays translucent.
pub const BACKING_INSET: f32 = 2.0;
/// A page's footer line (Weather's attribution, Prayer's Hijri date), in DIPs from the body's left and top
/// edges: x, y, width, height, the same on the plain and the media panel. Its bottom is 12 above the body's
/// bottom and it ends before the settings button.
const FOOTER: (f32, f32, f32, f32) = (PAD, 103.0, 300.0, 13.0);
pub fn footer(_media: bool) -> (f32, f32, f32, f32) {
    FOOTER
}

/// Whether a window point is on the settings button (a little slack around it); false while the panel is closed.
pub fn gear_at(window_width: i32, dpi: u32, progress: f32, media: bool, x: i32, y: i32) -> bool {
    if motion::reveal(progress) < 1.0 {
        return false;
    }
    let (g, slack) = (gear(dpi, media), scaled(6.0, dpi));
    let body = notch_body(window_width, dpi, notch_shape(progress, media));
    let x = x - body.left;
    (g.x - slack..g.x + g.width + slack).contains(&x)
        && (g.y - slack..g.y + g.height + slack).contains(&y)
}

/// The header row's one action (Limits' Refresh, Shelf's Add files), centred on the header's middle line and
/// ending 8 before the clock. Ends at y 29, above the Limits column labels (y 30).
const ACTION: (f32, f32, f32, f32) = (192.0, 9.0, 72.0, 20.0);
pub fn refresh(dpi: u32) -> Bounds {
    scaled_bounds(ACTION, dpi)
}

/// Whether a window point is on the Refresh button; false while the panel is closed.
pub fn refresh_at(window_width: i32, dpi: u32, progress: f32, media: bool, x: i32, y: i32) -> bool {
    on_slot(refresh(dpi), window_width, dpi, progress, media, x, y)
}
pub fn shelf_action(dpi: u32, undo: bool) -> Bounds {
    // Add files takes the header action; Undo sits in the grid's free cell (row two, right), short of the gear.
    scaled_bounds(
        if undo {
            (shelf_cell(1).0, shelf_cell(2).1, 140.0, SHELF_CELL_H)
        } else {
            ACTION
        },
        dpi,
    )
}

pub fn shelf_action_at(
    window_width: i32,
    dpi: u32,
    progress: f32,
    media: bool,
    (x, y): (i32, i32),
    undo: bool,
) -> bool {
    on_slot(
        shelf_action(dpi, undo),
        window_width,
        dpi,
        progress,
        media,
        x,
        y,
    )
}

/// The page buttons (the timer's presets, the empty Weather and Prayer "Set city"): 58 x 24 capsules 66
/// apart from x 16, y 68, in DIPs from the body's left and top edges, five across the content width.
/// `beside`: the Timer page's controls while a countdown shows, 56 x 24 capsules 64 apart from x 152 at y 40,
/// right of the digits.
pub fn page_button(index: usize, beside: bool, dpi: u32) -> Bounds {
    scaled_bounds(
        if beside {
            (152.0 + 64.0 * index as f32, 40.0, 56.0, CHIP_H)
        } else {
            (PAD + 66.0 * index as f32, 68.0, 58.0, CHIP_H)
        },
        dpi,
    )
}
/// The page button under a window point among the first `count`; none while the panel is closed.
pub fn page_button_at(
    window_width: i32,
    dpi: u32,
    progress: f32,
    media: bool,
    (count, beside): (usize, bool),
    (x, y): (i32, i32),
) -> Option<usize> {
    let vertical_padding = scaled(3.0, dpi);
    (0..count).find(|&i| {
        let mut target = page_button(i, beside, dpi);
        target.y -= vertical_padding;
        target.height += 2 * vertical_padding;
        on_slot(target, window_width, dpi, progress, media, x, y)
    })
}
fn on_slot(
    button: Bounds,
    window_width: i32,
    dpi: u32,
    progress: f32,
    media: bool,
    x: i32,
    y: i32,
) -> bool {
    if motion::reveal(progress) < 1.0 {
        return false;
    }
    let body = notch_body(window_width, dpi, notch_shape(progress, media));
    let x = x - body.left;
    (button.x..button.x + button.width).contains(&x)
        && (button.y..button.y + button.height).contains(&y)
}

/// The visualizer slot right of the controls row (slice 7, provisional), in DIPs from the body's
/// left and top edges: `BARS` bars `METER_BAR` wide, spread across it and bottom-aligned, ending 16
/// before the body's right edge and centred on the controls row.
const METER: (f32, f32, f32, f32) = (312.0, 32.0, 32.0, 20.0);
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
/// The title ends before the mascot's eyes (centred, 171 to 189) and the action slot starts after them.
const PAGE_TITLE: (f32, f32, f32, f32) = (PAD, HEAD_Y, 150.0, HEAD_H);
const PAGE_VALUE: (f32, f32, f32, f32) = (PAD, 34.0, 132.0, 36.0);
const PAGE_CLOCK: (f32, f32, f32, f32) = (0.0, HEAD_Y, 72.0, HEAD_H);
/// The mascot's resting eyes in the header, centred on the panel: their box in DIPs from the body's left edge.
pub const HEAD_EYES: (f32, f32) = (18.0, 18.0);

pub fn page_title(dpi: u32) -> Bounds {
    scaled_bounds(PAGE_TITLE, dpi)
}
pub fn page_value(dpi: u32) -> Bounds {
    scaled_bounds(PAGE_VALUE, dpi)
}
/// The panel's width on the current page (the 420 wide Widgets page, else the plain 360).
pub fn panel_width() -> f32 {
    if PAGE_HEIGHT.load(Relaxed) == 140 {
        WIDE_W
    } else {
        NOTCH_EXPANDED.width
    }
}
/// The empty shelf's dashed drop zone, the whole content width; it is the add slot's hit box too.
pub fn shelf_zone(dpi: u32) -> Bounds {
    scaled_bounds((PAD, BODY_Y, NOTCH_EXPANDED.width - 2.0 * PAD, 56.0), dpi)
}
/// The page clock, right-aligned 16 from the page's right edge.
pub fn page_clock(_media: bool, dpi: u32) -> Bounds {
    let (_, y, w, h) = PAGE_CLOCK;
    scaled_bounds((panel_width() - PAD - w, y, w, h), dpi)
}
/// The header row's rects on a page `width` wide, in DIPs: title, eyes box, action, clock. They never overlap.
#[cfg(test)]
pub fn header_rects(width: f32) -> [(f32, f32, f32, f32); 4] {
    let (_, y, w, h) = PAGE_CLOCK;
    [
        PAGE_TITLE,
        (
            width / 2.0 - HEAD_EYES.0 / 2.0,
            HEAD_Y,
            HEAD_EYES.0,
            HEAD_EYES.1,
        ),
        (
            width - PAD - w - GUTTER - ACTION.2,
            ACTION.1,
            ACTION.2,
            ACTION.3,
        ),
        (width - PAD - w, y, w, h),
    ]
}

/// The plain pill's minutes slot in pixels from the body's left and top edges.
pub fn timer_pill(dpi: u32) -> Bounds {
    scaled_bounds(TIMER_PILL, dpi)
}

/// Notice rows end before the Settings target. On Media the row starts below playback controls.
const MEDIA_NOTICE_ROOM: f32 = 28.0;
const POWER_NOTICE: [(f32, f32, f32, f32); 2] = [
    (12.0, NOTCH_EXPANDED.height - 16.0, 308.0, 14.0),
    (12.0, MEDIA_EXPANDED.height + 4.0, 308.0, 14.0),
];

/// The power pill's text slot in pixels from the body's left and top edges. A tall page (Limits) puts it in its
/// own row 6 under the page, like the plain panel, spanning the page's width.
pub fn power_notice(media: bool, dpi: u32) -> Bounds {
    notice_slot(media, PAGE_HEIGHT.load(Relaxed), dpi)
}
fn notice_slot(media: bool, page: u16, dpi: u32) -> Bounds {
    if page > 0 {
        let width = MEDIA_EXPANDED.width - 52.0;
        return scaled_bounds((12.0, f32::from(page) - 16.0, width, 14.0), dpi);
    }
    scaled_bounds(POWER_NOTICE[usize::from(media)], dpi)
}

/// The file shelf's `shelf::VISIBLE` file capsules in a two column grid under the header (156 x 28 DIPs, an 8
/// gutter), left to right then down, in DIPs from the body's left and top edges, on both panel kinds. Each draws
/// a 16 DIP system icon, the name and a remove target at its right end.
const SHELF_CELL_W: f32 = 156.0;
const SHELF_CELL_H: f32 = 28.0;
/// The origin of grid cell `index`.
fn shelf_cell(index: usize) -> (f32, f32) {
    (
        PAD + (SHELF_CELL_W + GUTTER) * (index % 2) as f32,
        BODY_Y + (SHELF_CELL_H + GUTTER) * (index / 2) as f32,
    )
}

/// A shelf slot in pixels from the body's left and top edges.
pub fn shelf_slot(_media: bool, index: usize, dpi: u32) -> Bounds {
    let (x, y) = shelf_cell(index);
    scaled_bounds((x, y, SHELF_CELL_W, SHELF_CELL_H), dpi)
}

/// A file capsule's remove target: its right-most 24 DIPs, in pixels from the body's left and top edges.
pub fn shelf_remove(index: usize, dpi: u32) -> Bounds {
    let (x, y) = shelf_cell(index);
    scaled_bounds((x + SHELF_CELL_W - 26.0, y, 26.0, SHELF_CELL_H), dpi)
}

/// The shelf slot whose remove target is under a window point; the caller ignores slots that are not files.
pub fn shelf_remove_at(
    window_width: i32,
    dpi: u32,
    progress: f32,
    x: i32,
    y: i32,
) -> Option<usize> {
    if motion::reveal(progress) < 1.0 {
        return None;
    }
    let body = notch_body(window_width, dpi, notch_shape(progress, false));
    (0..crate::shelf::VISIBLE).find(|&index| {
        let target = shelf_remove(index, dpi);
        (target.x..target.x + target.width).contains(&(x - body.left))
            && (target.y..target.y + target.height).contains(&y)
    })
}

/// The shelf slot under a window point at `progress`; none while the panel rows are not drawn. The caller
/// ignores slots past the entries it has.
pub fn shelf_at(
    window_width: i32,
    dpi: u32,
    progress: f32,
    media: bool,
    empty: bool,
    x: i32,
    y: i32,
) -> Option<usize> {
    if motion::reveal(progress) <= 0.0 {
        return None;
    }
    let body = notch_body(window_width, dpi, notch_shape(progress, media));
    // The empty shelf's whole drop zone is its one add slot.
    if empty {
        let zone = shelf_zone(dpi);
        return ((zone.x..zone.x + zone.width).contains(&(x - body.left))
            && (zone.y..zone.y + zone.height).contains(&y))
        .then_some(0);
    }
    (0..crate::shelf::VISIBLE).find(|&index| {
        let slot = shelf_slot(media, index, dpi);
        let (x, y) = (x - body.left - slot.x, y - slot.y);
        (0..slot.width).contains(&x) && (0..slot.height.min(body.height - slot.y)).contains(&y)
    })
}

/// The progress bar's target in the media panel (slice 4, provisional), in DIPs from the body's
/// left and top edges: the drawn 4 DIP bar is centred in it, between the elapsed and total times.
const MEDIA_PROGRESS: (f32, f32, f32, f32) = (148.0, 64.0, 156.0, 14.0);
/// The elapsed and total time slots either side of the bar, same row.
pub const MEDIA_TIMES: [(f32, f32, f32, f32); 2] =
    [(MEDIA_COLUMN, 64.0, 36.0, 14.0), (308.0, 64.0, 36.0, 14.0)];

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
        .enumerate()
        .position(|(index, control)| {
            let target = if index == 3 {
                scaled_bounds((240.0, 86.0, 32.0, 24.0), dpi)
            } else {
                control
            };
            panel_offset(window_width, dpi, progress, target, (x, y)).is_some()
        })
}

/// Where a window point falls along the progress bar, from 0 at its left edge towards 1.
pub fn media_seek_at(window_width: i32, dpi: u32, progress: f32, x: i32, y: i32) -> Option<f64> {
    let bar = media_progress(dpi);
    panel_offset(window_width, dpi, progress, bar, (x, y))
        .map(|offset| f64::from(offset) / f64::from(bar.width))
}

/// The tab handle (design round 2, T3 "Hanging handle", U5: always open): a bar hanging off the open panel's
/// bottom edge with one labelled cell per tab. `count` tabs are shown, `active` is the position of the
/// selected one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rail {
    pub count: usize,
    pub active: usize,
}

pub const RAIL_EARS: f32 = 6.0;
const RAIL_PAD_X: f32 = 8.0;
const RAIL_PAD_Y: f32 = 3.0;
/// A tab cell's width (fits the longest name, "Weather") and height (a glyph over the name), and the gap between cells.
const RAIL_CELL: (f32, f32) = (55.0, 34.0);
const RAIL_GAP: f32 = 2.0;

/// The handle's full size in DIPs: 413 x 40 for seven tabs.
pub fn rail_size(rail: Rail) -> (f32, f32) {
    let count = rail.count as f32;
    (
        2.0 * RAIL_PAD_X + count * RAIL_CELL.0 + (count - 1.0).max(0.0) * RAIL_GAP,
        2.0 * RAIL_PAD_Y + RAIL_CELL.1,
    )
}

/// The handle's bottom corner radius in pixels (a capsule's, 20 DIP), kept inside its shape.
pub fn rail_radius(shape: NotchBody, dpi: u32) -> f32 {
    scaled(20.0, dpi)
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

/// Handle item `index` in window pixels; they sit left to right inside the handle.
pub fn rail_item(handle: Bounds, dpi: u32, index: usize) -> Bounds {
    Bounds {
        x: handle.x + scaled(RAIL_PAD_X + index as f32 * (RAIL_CELL.0 + RAIL_GAP), dpi),
        y: handle.y + scaled(RAIL_PAD_Y, dpi),
        width: scaled(RAIL_CELL.0, dpi),
        height: scaled(RAIL_CELL.1, dpi),
    }
}

/// The tab position under a window point. The gap beside an item belongs to it, so a click between two
/// cells still lands; a point outside the item row hits none.
pub fn rail_tab_at(
    window_width: i32,
    dpi: u32,
    progress: f32,
    media: bool,
    rail: Rail,
    (x, y): (i32, i32),
) -> Option<usize> {
    let hang = rail_bounds(window_width, dpi, progress, media, rail)?;
    // The whole handle height is a target, not only the cells.
    if !(hang.y..hang.y + hang.height).contains(&y) {
        return None;
    }
    let half_gap = scaled(RAIL_GAP, dpi) / 2;
    (0..rail.count).find(|&index| {
        let item = rail_item(hang, dpi, index);
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

/// Scale the entire notch together so the outer tabs remain reachable on narrow displays.
pub fn notch_dpi(work: Bounds, monitor_dpi: u32, text_factor: f64) -> u32 {
    let requested = (f64::from(monitor_dpi) * text_factor).round() as u32;
    let width_limit = work.width.max(1) as u64 * 96 / NOTCH_WINDOW.0 as u64;
    // Reserve the two 12 DIP margins used by placement when computing the height budget.
    let height_limit = work.height.max(1) as u64 * 96 / (NOTCH_WINDOW.1 + 24) as u64;
    requested.min(width_limit.min(height_limit) as u32).max(1)
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
    // The attached dock is a full-width bar (slots left, buttons right), the floating one a centred pill.
    let width = if attached { work.width } else { width }.clamp(1, work.width.max(1));
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
            (225.0, 45.0, 22.5, 15.0)
        );
        // The media pill would pass its panel's width, so it stops there.
        assert_eq!(
            pill(MEDIA_NOTCH, MEDIA_EXPANDED, 150).width,
            MEDIA_EXPANDED.width
        );
    }
    #[test]
    fn buttons_widen_the_floating_dock_around_its_centre_and_the_attached_bar_spans_the_work_area()
    {
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
                assert_eq!((at.y, at.height), (plain.y, plain.height));
                if style == DockStyle::Attached {
                    // The attached bar spans the work area whatever the buttons.
                    assert_eq!((at.x, at.width), (work.x, work.width));
                    continue;
                }
                assert_eq!(at.width, dock::width_px(buttons, 5, 144));
                assert!(at.width > plain.width, "{style:?} {mask}");
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
            (work.x, work.width, floating.height)
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
        assert_eq!(attached_radius(at, 96), 32.0);
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
                height: 432
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
            (96, (440, 216), (262, 30), (360, 128)),
            (144, (660, 324), (393, 45), (540, 192)),
            (192, (880, 432), (524, 60), (720, 256)),
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
        // 360 x 128 panel centred in the 440 DIP window: body x 40..400, the transport row y 84..112.
        let at = |progress, x, y| media_control_at(440, 96, progress, x, y);
        assert_eq!(at(1.0, 148, 84), Some(0));
        assert_eq!(at(1.0, 179, 111), Some(0));
        assert_eq!(at(1.0, 180, 100), None); // The gap between previous and play/pause.
        assert_eq!(at(1.0, 186, 100), Some(1));
        assert_eq!(at(1.0, 233, 100), Some(1));
        assert_eq!(at(1.0, 240, 100), Some(2));
        assert_eq!(at(1.0, 271, 100), Some(2));
        assert_eq!(at(1.0, 272, 100), None);
        assert_eq!(at(1.0, 240, 83), None);
        assert_eq!(at(1.0, 240, 112), None);
        // Collapsed, swelling or barely open: the row is not drawn, so nothing is hit.
        for progress in [0.0, 0.07, 0.35] {
            assert_eq!(at(progress, 188, 15), None);
        }
        // Opening: only the part of a chip the body already shows is a target.
        let body = notch_body(440, 96, notch_shape(0.6, true));
        assert!(body.height > 84 && body.height < 112);
        let left = body.left + 146;
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
        // Body x 40..400 at 100 %: bar from 188 to 344, rows 64..78; controls from 84.
        let seek = |progress, x, y| media_seek_at(440, 96, progress, x, y);
        assert_eq!(seek(1.0, 188, 71), Some(0.0));
        assert_eq!(seek(1.0, 227, 64), Some(0.25));
        assert_eq!(seek(1.0, 343, 77), Some(155.0 / 156.0));
        for (x, y) in [(187, 71), (344, 71), (227, 63), (227, 78), (188, 100)] {
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
        assert!(body.height > 64 && body.height < 78);
        let left = body.left + 148;
        assert_eq!(seek(0.36, left, body.height - 1), Some(0.0));
        assert_eq!(seek(0.36, left, body.height), None);
    }

    #[test]
    fn a_drag_clamps_to_the_bar_and_the_gear_sits_clear_of_every_control() {
        // Body x 40..400: the bar spans 312..358. Left of it is silent, right of it full, whatever the row.
        let drag = |progress, x| volume_fraction(440, 96, progress, x);
        assert_eq!(drag(1.0, 0), Some(0.0));
        assert_eq!(drag(1.0, 312), Some(0.0));
        assert_eq!(drag(1.0, 357), Some(1.0));
        assert_eq!(drag(1.0, 435), Some(1.0));
        assert_eq!(drag(0.0, 200), None, "closed");
        let middle = drag(1.0, 334).unwrap();
        assert!((middle - 22.0 / 45.0).abs() < 1e-6);
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
                    assert!(
                        bar.x + bar.width + scaled(4.0, dpi) <= g.x - scaled(4.0, dpi),
                        "{dpi}"
                    );
                    assert!(media_controls(dpi).iter().all(|c| c.x + c.width <= g.x));
                }
            }
        }
    }

    /// Distance in DIPs from `(x, y)` (panel coordinates) to the glass backing's edge: positive inside.
    /// The backing is the open body inset by `BACKING_INSET`, its bottom corners rounded to the body's radius less that.
    fn backing_depth(shape: NotchShape, x: f32, y: f32) -> f32 {
        let (left, right, bottom) = (
            BACKING_INSET,
            shape.width - BACKING_INSET,
            shape.height - BACKING_INSET,
        );
        let r = shape.radius - BACKING_INSET;
        let straight = (x - left).min(right - x).min(bottom - y);
        let (cx, cy) = (x.clamp(left + r, right - r), bottom - r);
        if y > cy {
            r - ((x - cx).powi(2) + (y - cy).powi(2)).sqrt()
        } else {
            straight
        }
    }

    #[test]
    fn every_footer_and_the_gear_sit_six_dips_inside_the_glass_backing() {
        for media in [false, true] {
            let shape = if media {
                MEDIA_EXPANDED
            } else {
                NOTCH_EXPANDED
            };
            let (x, y, w, h) = footer(media);
            for corner in [(x, y), (x + w, y), (x, y + h), (x + w, y + h)] {
                assert!(
                    backing_depth(shape, corner.0, corner.1) >= 6.0,
                    "{media} footer {corner:?}"
                );
            }
            // The gear is a disc 8 DIPs in radius.
            let g = gear(96, media);
            let (cx, cy) = ((g.x + g.width / 2) as f32, (g.y + g.height / 2) as f32);
            assert!(backing_depth(shape, cx, cy) >= 6.0 + 8.0, "{media} gear");
        }
        // The check itself: a point on the bottom curve's edge is not inside.
        assert!(backing_depth(NOTCH_EXPANDED, 16.0, NOTCH_EXPANDED.height - 1.0) < 6.0);
    }

    #[test]
    fn a_notice_grows_the_plain_panel_by_its_row_and_leaves_padding() {
        assert_eq!(with_notice(92.0, 0.0), 92.0);
        assert_eq!(with_notice(92.0, 0.5), 96.0);
        assert_eq!(with_notice(92.0, 1.0), 92.0 + NOTICE_ROOM);
        assert_eq!(with_notice(92.0, 3.0), 92.0 + NOTICE_ROOM, "clamped");
        let (_, y, _, h) = (0.0, NOTCH_EXPANDED.height - 16.0, 0.0, 14.0);
        let notice = power_notice(false, 96);
        assert_eq!((notice.y as f32, notice.height as f32), (y, h));
        // Pills end at 92: well above the row; 10 below it once the panel has grown.
        let pill = page_button(0, false, 96);
        assert!(notice.y - (pill.y + pill.height) >= 6);
        assert_eq!(NOTCH_EXPANDED.height + NOTICE_ROOM - (y + h), 10.0);
        // Grown, with the rail below and the open overshoot, it still fits the window.
        let tallest = NOTCH.height + (NOTCH_EXPANDED.height + NOTICE_ROOM - NOTCH.height) * 1.07;
        assert!(
            tallest
                + rail_size(Rail {
                    count: 6,
                    active: 0
                })
                .1
                <= NOTCH_WINDOW.1 as f32
        );
    }

    #[test]
    fn the_limits_page_grows_with_its_rows_and_still_fits_the_window_with_the_rail() {
        assert_eq!(limits_height(3), 156);
        assert!(limits_height(2) < limits_height(3));
        // Three rows, open overshoot (progress 1.07) and the rail below stay inside the notch window.
        let shape = NOTCH_EXPANDED.height;
        let page = f32::from(limits_height(3));
        let tallest = NOTCH.height + (page.max(shape) - NOTCH.height) * 1.07;
        assert!(
            tallest
                + rail_size(Rail {
                    count: 6,
                    active: 0
                })
                .1
                <= NOTCH_WINDOW.1 as f32
        );
        // The last row ends above the gear's row.
        let last_end = LIMITS_TOP + LIMITS_ROW * 3.0;
        assert!(last_end <= page - 26.0 + 4.0, "{last_end}");
    }

    #[test]
    fn volume_bar_maps_its_first_and_last_pixel_to_silent_and_full() {
        // Body x 40..400: the bar spans 312..358 and 88..108 at 100 %, beside the mute toggle.
        let at = |progress, x, y| volume_at(440, 96, progress, x, y);
        assert_eq!(at(1.0, 312, 88), Some(0.0));
        assert_eq!(at(1.0, 357, 107), Some(1.0));
        assert_eq!(at(1.0, 312 + 22, 98), Some(22.0 / 45.0));
        for (x, y) in [(311, 98), (358, 98), (330, 87), (330, 108)] {
            assert_eq!(at(1.0, x, y), None, "{x}, {y}");
        }
        assert_eq!(media_control_at(440, 96, 1.0, 290, 100), Some(3));
        assert_eq!(media_control_at(440, 96, 1.0, 330, 100), None);
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
                height: scaled(NOTCH_WINDOW.1 as f32, dpi),
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
        assert!(!notch_hit(440, 96, 1.0, false, None, 30, 40));
        assert!(
            !notch_rows(
                Bounds {
                    x: 0,
                    y: 0,
                    width: 440,
                    height: NOTCH_WINDOW.1
                },
                96,
                1.0,
                false,
                None
            )
            .iter()
            .any(|r| r.y == 40 && r.x <= 30)
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
                    NotchShape {
                        height: MEDIA_EXPANDED.height + MEDIA_NOTICE_ROOM,
                        ..MEDIA_EXPANDED
                    }
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
                for slot in [title, value, clock, notice, shelf] {
                    assert!(fits(slot), "{dpi} {media}");
                }
                // Timer page: title, value, clock and the notice row never share a pixel.
                let timer_page = [title, value, clock, notice];
                for (a, first) in timer_page.iter().enumerate() {
                    for second in &timer_page[a + 1..] {
                        assert!(!hit(*first, *second), "{dpi} {media}");
                    }
                }
                // The Timer page's buttons, idle (five) or beside the digits (three), clear of the rest.
                for (count, beside) in [(5, false), (3, true)] {
                    for index in 0..count {
                        let button = page_button(index, beside, dpi);
                        assert!(fits(button), "{dpi} {media}");
                        for other in [title, clock, gear(dpi, media)] {
                            assert!(!hit(button, other), "{dpi} {media} {index}");
                        }
                        assert!(!beside || !hit(button, value), "{dpi} {media}");
                    }
                }
                // Shelf page: the first file capsule, the title and the clock.
                let shelf_page = [title, clock, shelf];
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
    fn notices_leave_room_for_the_settings_target() {
        for (media, page, width) in [
            (false, 0, NOTCH_EXPANDED.width),
            (true, 0, MEDIA_EXPANDED.width),
            (true, limits_height(3), MEDIA_EXPANDED.width),
        ] {
            for dpi in [96, 120, 144, 192, 240] {
                let notice = notice_slot(media, page, dpi);
                let settings_left = scaled(width - 16.0 - GEAR_SIZE, dpi) - scaled(6.0, dpi);
                assert!(
                    notice.x + notice.width < settings_left,
                    "{media} {page} {dpi}"
                );
            }
        }
    }

    #[test]
    fn a_tall_page_gets_its_notice_row_under_its_content_inside_the_grown_panel() {
        let page = limits_height(3);
        let notice = notice_slot(true, page, 96);
        // 6 under the last row, 10 above the grown panel's bottom.
        assert_eq!(notice.y, i32::from(page) - 16);
        assert!(
            notice.y as f32 >= LIMITS_TOP + LIMITS_ROW * 3.0 + 6.0,
            "6 under the last row"
        );
        assert_eq!(
            f32::from(page) + NOTICE_ROOM - (notice.y + notice.height) as f32,
            10.0
        );
        assert!(notice.x + notice.width <= MEDIA_EXPANDED.width as i32);
        assert_eq!(
            notice_slot(false, 0, 96),
            power_notice(false, 96),
            "plain panel unchanged"
        );
        // Grown by the notice, with the open overshoot and the rail below, it stays inside the window.
        let tallest = NOTCH.height + (f32::from(page) + NOTICE_ROOM - NOTCH.height) * 1.07;
        assert!(
            tallest
                + rail_size(Rail {
                    count: 6,
                    active: 0
                })
                .1
                <= NOTCH_WINDOW.1 as f32
        );
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
            assert!(
                fits(
                    media,
                    NotchShape {
                        height: MEDIA_EXPANDED.height + MEDIA_NOTICE_ROOM,
                        ..MEDIA_EXPANDED
                    }
                ),
                "{dpi}"
            );
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
                    assert_eq!(shelf_at(width, dpi, 1.0, media, false, x, y), Some(index));
                    assert_eq!(media_control_at(width, dpi, 1.0, x, y), None);
                    assert_eq!(media_seek_at(width, dpi, 1.0, x, y), None);
                    assert_eq!(volume_at(width, dpi, 1.0, x, y), None);
                    // Just outside either edge, and above or below.
                    assert_ne!(
                        shelf_at(width, dpi, 1.0, media, false, body.left + slot.x - 1, y),
                        Some(index)
                    );
                    assert_eq!(shelf_at(width, dpi, 1.0, media, false, x, slot.y - 1), None);
                    assert_eq!(
                        shelf_at(width, dpi, 1.0, media, false, x, slot.y + slot.height),
                        None
                    );
                    // Not drawn while the panel is closed.
                    assert_eq!(shelf_at(width, dpi, 0.0, media, false, x, y), None);
                }
            }
        }
    }

    fn rail(count: usize, active: usize) -> Rail {
        Rail { count, active }
    }

    #[test]
    fn scaled_notch_keeps_the_seven_tab_rail_on_narrow_work_areas() {
        let work = Bounds {
            x: 0,
            y: 0,
            width: 1024,
            height: 768,
        };
        assert_eq!(notch_dpi(work, 96, 1.5), 144);
        let dpi = notch_dpi(work, 192, 2.25);
        let window = placement(work, dpi, SurfaceKind::Notch, 0);
        let rail = rail(7, 0);
        let row = rail_bounds(window.width, dpi, 1.0, true, rail).unwrap();
        for index in 0..rail.count {
            let cell = rail_item(row, dpi, index);
            assert!(cell.x >= 0 && cell.x + cell.width <= window.width);
            assert!(cell.y >= 0 && cell.y + cell.height <= window.height);
        }
    }

    #[test]
    fn the_handle_reserves_label_width_for_all_seven_tabs() {
        let (six_w, six_h) = rail_size(rail(6, 0));
        assert_eq!((six_w, six_h), (356.0, 40.0));
        let (seven_w, seven_h) = rail_size(rail(7, 0));
        assert_eq!((seven_w, seven_h), (413.0, 40.0));
        assert!(seven_w < NOTCH_WINDOW.0 as f32);
        assert!(rail_size(rail(3, 0)).0 < rail_size(rail(5, 0)).0);
    }

    #[test]
    fn the_handle_hangs_from_the_open_panel_and_stays_inside_the_window() {
        for dpi in [96, 144, 192, 240] {
            let window = Bounds {
                x: 0,
                y: 0,
                width: scaled(440.0, dpi),
                height: scaled(NOTCH_WINDOW.1 as f32, dpi),
            };
            for media in [false, true] {
                let r = rail(7, 2);
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
                    // Centred like the body (to a pixel).
                    assert!((hang.x + hang.width / 2).abs_diff(window.width / 2) <= 1);
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
        assert_eq!(rail_bounds(window, 96, 1.0, true, rail(1, 0)), None);
        assert_eq!(rail_bounds(window, 96, 1.0, true, rail(0, 0)), None);
        let rows = notch_rows(
            Bounds {
                x: 0,
                y: 0,
                width: window,
                height: NOTCH_WINDOW.1,
            },
            96,
            1.0,
            true,
            Some(rail(1, 0)),
        );
        assert_eq!(rows.len(), 128, "just the panel");
    }

    #[test]
    fn handle_items_lie_inside_it_in_order_and_each_point_picks_one_tab() {
        for dpi in [96, 144, 192] {
            let width = scaled(440.0, dpi);
            let r = rail(6, 1);
            let hang = rail_bounds(width, dpi, 1.0, true, r).unwrap();
            let items: Vec<Bounds> = (0..6).map(|i| rail_item(hang, dpi, i)).collect();
            for (i, item) in items.iter().enumerate() {
                assert!(
                    item.x >= hang.x && item.x + item.width <= hang.x + hang.width,
                    "{dpi} {i}"
                );
                assert!(
                    item.y >= hang.y && item.y + item.height <= hang.y + hang.height,
                    "{dpi} {i}"
                );
                assert!(
                    i == 0 || item.x >= items[i - 1].x + items[i - 1].width,
                    "ordered, no overlap"
                );
                let middle = (item.x + item.width / 2, item.y + item.height / 2);
                assert_eq!(rail_tab_at(width, dpi, 1.0, true, r, middle), Some(i));
            }
            // Above the item row, or outside the handle, picks nothing.
            assert_eq!(
                rail_tab_at(width, dpi, 1.0, true, r, (items[2].x, hang.y - 1)),
                None
            );
            assert_eq!(
                rail_tab_at(width, dpi, 1.0, true, r, (hang.x - 1, items[0].y)),
                None
            );
        }
    }

    type Rect = (f32, f32, f32, f32);
    fn overlap(a: Rect, b: Rect) -> bool {
        a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
    }
    fn dips(slot: Bounds, dpi: u32) -> Rect {
        let k = dpi as f32 / 96.0;
        (
            slot.x as f32 / k,
            slot.y as f32 / k,
            slot.width as f32 / k,
            slot.height as f32 / k,
        )
    }

    #[test]
    fn the_header_row_is_one_grid_with_no_overlap_on_every_page_width() {
        for width in [NOTCH_EXPANDED.width, WIDE_W] {
            let rects = header_rects(width);
            for (a, first) in rects.iter().enumerate() {
                // Inside the panel's margins and above the content.
                assert!(
                    first.0 >= PAD && first.0 + first.2 <= width - PAD + 0.01,
                    "{width} {a}"
                );
                assert!(first.1 >= 0.0 && first.1 + first.3 <= BODY_Y, "{width} {a}");
                for second in &rects[a + 1..] {
                    assert!(!overlap(*first, *second), "{width}: {first:?} {second:?}");
                }
            }
        }
    }

    #[test]
    fn limits_refresh_clears_the_header_and_the_column_labels() {
        // The Limits page's own header: title, clock, and the window labels (y 30, 12 high) under it.
        let labels: [Rect; 2] = [(96.0, 30.0, 100.0, 12.0), (224.0, 30.0, 100.0, 12.0)];
        for dpi in [96, 120, 144, 192, 240] {
            let button = dips(refresh(dpi), dpi);
            let title = dips(page_title(dpi), dpi);
            let clock = dips(page_clock(true, dpi), dpi);
            let first_row = LIMITS_TOP;
            // Close to exact at every scale (whole-pixel rounding).
            let near = |a: f32, b: f32| (a - b).abs() < 1.0;
            assert!(near(button.1 + button.3, 29.0), "{dpi} {button:?}");
            for other in [title, clock].into_iter().chain(labels) {
                assert!(
                    !overlap(
                        (
                            button.0 + 0.5,
                            button.1 + 0.5,
                            button.2 - 1.0,
                            button.3 - 1.0
                        ),
                        other
                    ),
                    "{dpi}: {button:?} against {other:?}"
                );
            }
            assert!(labels.iter().all(|label| label.1 + label.3 <= first_row));
            // The first row's text starts under the button too.
            assert!(button.1 + button.3 <= first_row);
        }
        // The Limits panel is 360 wide: the button also clears the page clock's slot there.
        assert!(!overlap(ACTION, header_rects(360.0)[3]));
    }

    #[test]
    fn the_media_panel_places_cover_text_progress_transport_and_gear_without_overlap() {
        let panel = |shape: NotchShape| (0.0, 0.0, shape.width, shape.height);
        let controls = MEDIA_CONTROLS;
        let mut rects: Vec<Rect> = controls.to_vec();
        rects.extend([
            VOLUME_BAR,
            MEDIA_PROGRESS,
            MEDIA_TIMES[0],
            MEDIA_TIMES[1],
            METER,
        ]);
        let g = gear(96, true);
        rects.push((g.x as f32, g.y as f32, g.width as f32, g.height as f32));
        for (a, first) in rects.iter().enumerate() {
            let outer = panel(MEDIA_EXPANDED);
            assert!(
                first.0 >= 0.0
                    && first.1 >= 0.0
                    && first.0 + first.2 <= outer.2
                    && first.1 + first.3 <= outer.3,
                "{first:?}"
            );
            for second in &rects[a + 1..] {
                assert!(!overlap(*first, *second), "{first:?} {second:?}");
            }
        }
    }

    #[test]
    fn shelf_capsules_form_a_grid_whose_remove_targets_are_their_own() {
        let window = scaled(440.0, 96);
        for index in 0..crate::shelf::VISIBLE {
            let cell = shelf_slot(false, index, 96);
            let cross = shelf_remove(index, 96);
            assert!(cross.x >= cell.x && cross.x + cross.width <= cell.x + cell.width);
            assert!(cross.y == cell.y && cross.height == cell.height);
            let body = notch_body(window, 96, notch_shape(1.0, false));
            let (x, y) = (
                body.left + cross.x + cross.width / 2,
                cross.y + cross.height / 2,
            );
            assert_eq!(shelf_remove_at(window, 96, 1.0, x, y), Some(index));
            // The capsule's name side is not a remove target.
            assert_eq!(
                shelf_remove_at(window, 96, 1.0, body.left + cell.x + 40, y),
                None
            );
            // Undo takes the grid's free cell and clears the gear.
            let undo = shelf_action(96, true);
            assert!(!hit(undo, cell) || index == 3);
            assert!(!hit(undo, gear(96, false)));
        }
        // An empty shelf is one zone: any point in it adds.
        let zone = shelf_zone(96);
        let body = notch_body(window, 96, notch_shape(1.0, false));
        let (x, y) = (
            body.left + zone.x + zone.width - 4,
            zone.y + zone.height - 4,
        );
        assert_eq!(shelf_at(window, 96, 1.0, false, true, x, y), Some(0));
        assert_eq!(shelf_at(window, 96, 1.0, false, true, x, zone.y - 2), None);
    }
}
