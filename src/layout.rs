use crate::{dock, motion};

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
pub fn notch_rows(window: Bounds, dpi: u32, progress: f32, media: bool) -> Vec<Bounds> {
    let shape = notch_shape(progress, media);
    let body = notch_body(window.width, dpi, shape);
    let radius = notch_radius(body, shape, dpi);
    (0..body.height.min(window.height))
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
        .collect()
}

/// Hover follows the silhouette with four DIP of slop; transparent side bands do not hold it open.
pub fn notch_hit(window_width: i32, dpi: u32, progress: f32, media: bool, x: i32, y: i32) -> bool {
    let shape = notch_shape(progress, media);
    let body = notch_body(window_width, dpi, shape);
    let slop = scaled(NOTCH_HOVER_SLOP, dpi);
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
/// height. The visualizer slot right of the controls row stays empty.
const MEDIA_CONTROLS: [(f32, f32, f32, f32); 4] = [
    (100.0, 66.0, 28.0, 32.0),
    (148.0, 66.0, 32.0, 32.0),
    (200.0, 66.0, 28.0, 32.0),
    (100.0, 100.0, 24.0, 16.0),
];
/// The volume level bar's hit box, right of the mute toggle and ending 16 before the body's right.
const VOLUME_BAR: (f32, f32, f32, f32) = (132.0, 100.0, 212.0, 16.0);

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

/// `dock_slots` sizes the dock (picker slot plus app groups); the notch ignores it.
pub fn placement(work: Bounds, dpi: u32, kind: SurfaceKind, dock_slots: usize) -> Bounds {
    let scale = |dips: i32| dock::px(dips, dpi);
    let (width, height) = match kind {
        SurfaceKind::Notch => (scale(NOTCH_WINDOW.0), scale(NOTCH_WINDOW.1)),
        SurfaceKind::Dock => (
            scale(dock::width_dips(dock_slots)),
            scale(dock::HEIGHT_DIPS),
        ),
    };
    let margin = scale(12).min(work.height.max(1) / 4);
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
    fn volume_bar_maps_its_first_and_last_pixel_to_silent_and_full() {
        // Body x 40..400: the bar spans 172..384 and 100..116 at 100 %, below the mute toggle's row.
        let at = |progress, x, y| volume_at(440, 96, progress, x, y);
        assert_eq!(at(1.0, 172, 100), Some(0.0));
        assert_eq!(at(1.0, 383, 115), Some(1.0));
        assert_eq!(at(1.0, 172 + 211 / 2, 108), Some(105.0 / 211.0));
        for (x, y) in [(171, 108), (384, 108), (200, 99), (200, 116)] {
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
                let rows = notch_rows(window, dpi, progress, media);
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
                    side_x,
                    side_y
                ));
                assert!(!contains(body.left, body.height - 1));
                assert!(!notch_hit(
                    window.width,
                    dpi,
                    progress,
                    media,
                    body.left - scaled(5.0, dpi),
                    body.height - 1
                ));
                assert!(!contains(middle, body.height + scaled(10.0, dpi)));
            }
        }
        // Independent review's concrete regression point, 10 px beside the plain panel, moved with
        // the window's 20 px wider sides.
        assert!(!notch_hit(440, 96, 1.0, false, 50, 40));
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
                false
            )
            .iter()
            .any(|r| r.y == 40 && r.x <= 50)
        );
    }
}
