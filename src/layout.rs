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
/// The notch HWND never moves or resizes: the expanded body and ears plus room for the open
/// overshoot and the soft shadow. `notch_region` keeps input and pixels to what is actually drawn.
pub const NOTCH_WINDOW: (i32, i32) = (400, 124);
/// How far the shadow reaches past the open body; the region grows into it as the notch opens.
const NOTCH_SHADOW_REACH: f32 = 44.0;
/// Pointer slop around the drawn body that still counts as hovering it.
const NOTCH_HOVER_SLOP: f32 = 4.0;

/// Shape at an animation progress: size follows the spring's overshoot; corners and ears never overshoot.
pub fn notch_shape(progress: f32) -> NotchShape {
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let settled = progress.clamp(0.0, 1.0);
    NotchShape {
        width: lerp(NOTCH.width, NOTCH_EXPANDED.width, progress),
        height: lerp(NOTCH.height, NOTCH_EXPANDED.height, progress),
        radius: lerp(NOTCH.radius, NOTCH_EXPANDED.radius, settled),
        ears: lerp(NOTCH.ears, NOTCH_EXPANDED.ears, settled),
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

/// Window-relative input/visible region: body and ears, plus shadow room that grows with the open
/// progress. At rest it is exactly the collapsed footprint; transparent margins never take clicks
/// unless the shadow is drawn there.
pub fn notch_region(window: Bounds, dpi: u32, progress: f32) -> Bounds {
    let body = notch_body(window.width, dpi, notch_shape(progress));
    let reach = scaled(NOTCH_SHADOW_REACH * motion::smooth(progress).sqrt(), dpi);
    let side = body.ears.max(reach);
    let left = (body.left - side).max(0);
    let right = (body.left + body.width + side).min(window.width);
    Bounds {
        x: left,
        y: 0,
        width: right - left,
        height: (body.height + reach).min(window.height),
    }
}

/// Whether a window-relative point is over the drawn body or ears (plus slop); shadow and margins never hold the hover.
pub fn notch_hit(window_width: i32, dpi: u32, progress: f32, x: i32, y: i32) -> bool {
    let body = notch_body(window_width, dpi, notch_shape(progress));
    let reach = body.ears + scaled(NOTCH_HOVER_SLOP, dpi);
    (0..body.height + reach - body.ears).contains(&y)
        && (body.left - reach..body.left + body.width + reach).contains(&x)
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
                x: 1520,
                y: 0,
                width: 800,
                height: 248
            }
        );
    }

    #[test]
    fn notch_region_is_the_footprint_at_rest_and_grows_into_the_shadow_when_open() {
        let window = Bounds {
            x: 0,
            y: 0,
            width: 800,
            height: 248,
        };
        // 200 %: collapsed body 300x60 plus 20px ears, centred.
        assert_eq!(
            notch_region(window, 192, 0.0),
            Bounds {
                x: 230,
                y: 0,
                width: 340,
                height: 60
            }
        );
        // Open: the shadow reach fills the whole fixed window.
        assert_eq!(notch_region(window, 192, 1.0), window);
        // Hover intent swell: only slightly larger than the pill, never the transparent margins.
        let swell = notch_region(window, 192, 0.07);
        assert!(swell.width < 380 && swell.height < 92, "{swell:?}");
        // Hover follows the drawn shape: pill centre yes, side margin and shadow band no.
        assert!(notch_hit(800, 192, 0.0, 400, 20));
        assert!(!notch_hit(800, 192, 0.0, 120, 20));
        assert!(notch_hit(800, 192, 1.0, 400, 150));
        assert!(!notch_hit(800, 192, 1.0, 400, 200));
        assert!(!notch_hit(800, 192, 1.0, 20, 20));
    }
}
