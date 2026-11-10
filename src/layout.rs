use crate::dock;

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
/// While a media session exists the pill widens for the cover slot and title, and the panel grows
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
