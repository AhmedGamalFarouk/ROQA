//! Direct2D drawing for the notch tabs and the mascot (features tabs-eyes): the hanging handle with its five
//! icons, the pages other than Media, the opacity layer that fades the pill out as a page fades in, and the
//! squash-capsule eyes. Icons are small paths on a 16-unit grid drawn in the theme and accent colours; there
//! are no asset files. Nothing here runs unless the host puts a rail, a page or eyes in the scene.

use super::*;
use crate::{
    eyes::{self, Eye},
    layout::NotchBody,
    tabs::Tab,
    weather::Icon,
};
use windows::Win32::Graphics::Direct2D::{
    Common::{D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN},
    D2D1_ARC_SIZE_LARGE, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID, D2D1_ELLIPSE,
    D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1, D2D1_LINE_JOIN_ROUND,
    D2D1_STROKE_STYLE_PROPERTIES,
};

/// Draws `draw` at `opacity` (a layer), or directly at full opacity. The layer exists only for the open and
/// close frames, where the pill and a page cross-fade.
pub(super) fn faded(
    context: &ID2D1DeviceContext,
    opacity: f32,
    draw: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if opacity >= 1.0 {
        return draw();
    }
    if opacity <= 0.0 {
        return Ok(());
    }
    let parameters = D2D1_LAYER_PARAMETERS1 {
        contentBounds: D2D_RECT_F {
            left: f32::MIN,
            top: f32::MIN,
            right: f32::MAX,
            bottom: f32::MAX,
        },
        geometricMask: ManuallyDrop::new(None),
        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
        maskTransform: Matrix3x2::identity(),
        opacity,
        opacityBrush: ManuallyDrop::new(None),
        layerOptions: D2D1_LAYER_OPTIONS1_NONE,
    };
    // SAFETY: context is live between surface BeginDraw/EndDraw; the layer is popped below on the same
    // context, whatever `draw` returns, and the parameters hold no COM references.
    unsafe { context.PushLayer(&parameters, None) };
    let drawn = draw();
    // SAFETY: balances the PushLayer above.
    unsafe { context.PopLayer() };
    drawn
}

/// One path segment on the 16-unit icon grid.
#[derive(Clone, Copy)]
enum Seg {
    M(f32, f32),
    L(f32, f32),
    /// Arc to (x, y) with radius r: clockwise, large.
    A(f32, f32, f32, bool, bool),
    Z,
}
use Seg::{A, L, M, Z};

struct Art {
    /// Segments, and whether the closed figure is filled as well as stroked.
    paths: &'static [(&'static [Seg], bool)],
    /// Centre, radius and whether it is filled.
    circles: &'static [(f32, f32, f32, bool)],
}

const MEDIA: Art = Art {
    paths: &[(
        &[
            M(6.4, 11.4),
            L(6.4, 3.1),
            L(13.4, 1.6),
            L(13.4, 9.9),
            M(6.4, 5.3),
            L(13.4, 3.8),
        ],
        false,
    )],
    circles: &[(4.6, 11.8, 1.9, true), (11.6, 10.2, 1.9, true)],
};
const WEATHER: Art = Art {
    paths: &[
        (
            &[
                M(5.0, 0.9),
                L(5.0, 1.9),
                M(1.0, 5.0),
                L(2.0, 5.0),
                M(2.2, 2.2),
                L(2.9, 2.9),
                M(7.8, 2.2),
                L(7.1, 2.9),
            ],
            false,
        ),
        (
            &[
                M(5.6, 14.2),
                L(12.2, 14.2),
                A(12.5, 9.4, 2.4, false, false),
                A(5.6, 10.3, 3.6, false, false),
                A(5.6, 14.2, 2.1, false, false),
                Z,
            ],
            false,
        ),
    ],
    circles: &[(5.0, 5.0, 1.9, false)],
};
const PRAYER: Art = Art {
    paths: &[
        (
            &[
                M(8.0, 2.2),
                A(14.0, 10.4, 6.0, false, true),
                A(8.0, 2.2, 4.9, true, false),
                Z,
            ],
            false,
        ),
        (
            &[
                M(12.6, 1.8),
                L(13.1, 3.0),
                L(14.3, 3.5),
                L(13.1, 4.0),
                L(12.6, 5.2),
                L(12.1, 4.0),
                L(10.9, 3.5),
                L(12.1, 3.0),
                Z,
            ],
            true,
        ),
    ],
    circles: &[],
};
const TIMER: Art = Art {
    paths: &[(
        &[
            M(8.0, 9.2),
            L(8.0, 6.4),
            M(6.0, 1.6),
            L(10.0, 1.6),
            M(12.3, 4.3),
            L(13.3, 3.3),
        ],
        false,
    )],
    circles: &[(8.0, 9.2, 5.2, false)],
};
const SHELF: Art = Art {
    paths: &[
        (
            &[
                M(1.8, 9.4),
                L(3.5, 3.8),
                A(4.5, 3.0, 1.0, true, false),
                L(11.5, 3.0),
                A(12.5, 3.8, 1.0, true, false),
                L(14.2, 9.4),
                L(14.2, 12.4),
                A(13.2, 13.4, 1.0, true, false),
                L(2.8, 13.4),
                A(1.8, 12.4, 1.0, true, false),
                Z,
            ],
            false,
        ),
        (
            &[
                M(1.8, 9.4),
                L(5.6, 9.4),
                A(10.4, 9.4, 2.4, false, false),
                L(14.2, 9.4),
            ],
            false,
        ),
    ],
    circles: &[],
};

// The Weather page's pictures (features W1), on the same grid. A cloud is the Weather tab's own outline, moved.
const SUN: Art = Art {
    paths: &[(
        &[
            M(8.0, 1.4),
            L(8.0, 2.8),
            M(8.0, 13.2),
            L(8.0, 14.6),
            M(1.4, 8.0),
            L(2.8, 8.0),
            M(13.2, 8.0),
            L(14.6, 8.0),
            M(3.3, 3.3),
            L(4.4, 4.4),
            M(12.7, 3.3),
            L(11.6, 4.4),
            M(3.3, 12.7),
            L(4.4, 11.6),
            M(12.7, 12.7),
            L(11.6, 11.6),
        ],
        false,
    )],
    circles: &[(8.0, 8.0, 2.9, false)],
};
const CLOUD: Art = Art {
    paths: &[(
        &[
            M(4.6, 12.2),
            L(11.2, 12.2),
            A(11.5, 7.4, 2.4, false, false),
            A(4.6, 8.3, 3.6, false, false),
            A(4.6, 12.2, 2.1, false, false),
            Z,
        ],
        false,
    )],
    circles: &[],
};
const FOG: Art = Art {
    paths: &[(
        &[
            M(3.0, 4.5),
            L(13.0, 4.5),
            M(2.0, 7.5),
            L(12.0, 7.5),
            M(4.0, 10.5),
            L(14.0, 10.5),
            M(3.0, 13.5),
            L(11.0, 13.5),
        ],
        false,
    )],
    circles: &[],
};
const DRIZZLE: Art = Art {
    paths: &[
        (
            &[
                M(4.6, 9.7),
                L(11.2, 9.7),
                A(11.5, 4.9, 2.4, false, false),
                A(4.6, 5.8, 3.6, false, false),
                A(4.6, 9.7, 2.1, false, false),
                Z,
            ],
            false,
        ),
        (
            &[M(6.0, 12.0), L(5.6, 13.4), M(9.6, 12.0), L(9.2, 13.4)],
            false,
        ),
    ],
    circles: &[],
};
const RAIN: Art = Art {
    paths: &[
        (
            &[
                M(4.6, 9.7),
                L(11.2, 9.7),
                A(11.5, 4.9, 2.4, false, false),
                A(4.6, 5.8, 3.6, false, false),
                A(4.6, 9.7, 2.1, false, false),
                Z,
            ],
            false,
        ),
        (
            &[
                M(5.6, 11.6),
                L(4.8, 14.4),
                M(8.6, 11.6),
                L(7.8, 14.4),
                M(11.6, 11.6),
                L(10.8, 14.4),
            ],
            false,
        ),
    ],
    circles: &[],
};
const SNOW: Art = Art {
    paths: &[(
        &[
            M(4.6, 9.7),
            L(11.2, 9.7),
            A(11.5, 4.9, 2.4, false, false),
            A(4.6, 5.8, 3.6, false, false),
            A(4.6, 9.7, 2.1, false, false),
            Z,
        ],
        false,
    )],
    circles: &[
        (5.6, 12.8, 0.3, true),
        (8.8, 13.9, 0.3, true),
        (12.0, 12.8, 0.3, true),
    ],
};
const STORM: Art = Art {
    paths: &[
        (
            &[
                M(4.6, 9.7),
                L(11.2, 9.7),
                A(11.5, 4.9, 2.4, false, false),
                A(4.6, 5.8, 3.6, false, false),
                A(4.6, 9.7, 2.1, false, false),
                Z,
            ],
            false,
        ),
        (
            &[M(9.0, 10.8), L(6.8, 13.2), L(9.4, 13.2), L(7.2, 15.6)],
            false,
        ),
    ],
    circles: &[],
};

fn weather_art(icon: Icon) -> &'static Art {
    match icon {
        Icon::Clear => &SUN,
        Icon::PartlyCloudy => &WEATHER,
        Icon::Cloud => &CLOUD,
        Icon::Fog => &FOG,
        Icon::Drizzle => &DRIZZLE,
        Icon::Rain => &RAIN,
        Icon::Snow => &SNOW,
        Icon::Storm => &STORM,
    }
}

fn art(tab: Tab) -> &'static Art {
    match tab {
        Tab::Media => &MEDIA,
        Tab::Weather => &WEATHER,
        Tab::Prayer => &PRAYER,
        Tab::Timer => &TIMER,
        Tab::Shelf => &SHELF,
    }
}

/// Strokes `tab`'s icon (1.5 of 16 units, round caps and joins) in the `square`, filling the marked parts.
fn icon(
    context: &ID2D1DeviceContext,
    tab: Tab,
    square: D2D_RECT_F,
    color: D2D1_COLOR_F,
) -> Result<()> {
    draw_art(context, art(tab), square, color)
}

fn draw_art(
    context: &ID2D1DeviceContext,
    art: &Art,
    square: D2D_RECT_F,
    color: D2D1_COLOR_F,
) -> Result<()> {
    let unit = (square.right - square.left) / 16.0;
    let at = |x: f32, y: f32| Vector2 {
        X: square.left + x * unit,
        Y: square.top + y * unit,
    };
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush, stroke style, geometry and sink
    // are local COM objects scoped to this draw, and each sink is closed before its geometry is used.
    unsafe {
        let factory = context.GetFactory()?;
        let brush = context.CreateSolidColorBrush(&color, None)?;
        let style = factory.CreateStrokeStyle(
            &D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 1.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
            },
            None,
        )?;
        let stroke = 1.5 * unit;
        for &(segments, filled) in art.paths {
            let path = factory.CreatePathGeometry()?;
            let sink = path.Open()?;
            let mut open = false;
            for &segment in segments {
                match segment {
                    M(x, y) => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        let begin = if filled {
                            D2D1_FIGURE_BEGIN_FILLED
                        } else {
                            D2D1_FIGURE_BEGIN_HOLLOW
                        };
                        sink.BeginFigure(at(x, y), begin);
                        open = true;
                    }
                    L(x, y) => sink.AddLine(at(x, y)),
                    A(x, y, radius, clockwise, large) => sink.AddArc(&D2D1_ARC_SEGMENT {
                        point: at(x, y),
                        size: D2D_SIZE_F {
                            width: radius * unit,
                            height: radius * unit,
                        },
                        rotationAngle: 0.0,
                        sweepDirection: if clockwise {
                            D2D1_SWEEP_DIRECTION_CLOCKWISE
                        } else {
                            D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                        },
                        arcSize: if large {
                            D2D1_ARC_SIZE_LARGE
                        } else {
                            D2D1_ARC_SIZE_SMALL
                        },
                    }),
                    Z => {
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        open = false;
                    }
                }
            }
            if open {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            sink.Close()?;
            if filled {
                context.FillGeometry(&path, &brush, None);
            }
            context.DrawGeometry(&path, &brush, stroke, Some(&style));
        }
        for &(x, y, radius, filled) in art.circles {
            let circle = D2D1_ELLIPSE {
                point: at(x, y),
                radiusX: radius * unit,
                radiusY: radius * unit,
            };
            if filled {
                context.FillEllipse(&circle, &brush);
            }
            context.DrawEllipse(&circle, &brush, stroke, Some(&style));
        }
    }
    Ok(())
}

/// The hanging handle (design T3), drawn after the body clip is popped because it lies below the body: a
/// body-coloured shape with the notch's own concave ears, then per tab a dot that becomes an icon as the
/// handle opens (`p`), the active one accented, and a ring around the one with keyboard focus.
pub(super) fn rail(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    handle: &RailScene,
    surface: D2D_RECT_F,
    p: &Palette,
) -> Result<()> {
    let scale = scene.dpi as f32 / 96.0;
    let width = surface.right - surface.left;
    let media = scene.track.is_some();
    let Some(hang) = layout::rail_bounds(
        width as i32,
        scene.dpi,
        scene.progress,
        media,
        handle.layout,
    ) else {
        return Ok(());
    };
    let open = handle.layout.p.clamp(0.0, 1.0);
    let shape = D2D_RECT_F {
        left: surface.left + hang.x as f32,
        top: surface.top + hang.y as f32,
        right: surface.left + (hang.x + hang.width) as f32,
        bottom: surface.top + (hang.y + hang.height) as f32,
    };
    let ears = (layout::RAIL_EARS * scale).round().min(hang.height as f32);
    let radius = layout::rail_radius(
        NotchBody {
            left: hang.x,
            width: hang.width,
            height: hang.height,
            ears: ears as i32,
        },
        open,
        scene.dpi,
    );
    let outline = notch_outline(context, &shape, radius, ears)?;
    // SAFETY: context is live between surface BeginDraw/EndDraw; brushes and geometry are scoped to this draw.
    unsafe {
        let fill = context.CreateSolidColorBrush(&color(p.body), None)?;
        context.FillGeometry(&outline, &fill, None);
        // While the handle grows out of the panel its items stay inside it.
        context.PushAxisAlignedClip(&shape, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
    }
    let drawn = items(context, scene, handle, hang, surface, p);
    // SAFETY: balances the PushAxisAlignedClip above on the same live context.
    unsafe { context.PopAxisAlignedClip() };
    drawn
}

/// The handle's dots, icons, active wash and focus ring.
fn items(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    handle: &RailScene,
    hang: Bounds,
    surface: D2D_RECT_F,
    p: &Palette,
) -> Result<()> {
    let scale = scene.dpi as f32 / 96.0;
    let reveal = motion::reveal(scene.progress);
    let open = handle.layout.p.clamp(0.0, 1.0);
    for (position, &tab) in handle.tabs.iter().enumerate() {
        let item = layout::rail_item(hang, scene.dpi, handle.layout, position);
        let cell = D2D_RECT_F {
            left: surface.left + item.x as f32,
            top: surface.top + item.y as f32,
            right: surface.left + (item.x + item.width) as f32,
            bottom: surface.top + (item.y + item.height) as f32,
        };
        let active = position == handle.layout.active;
        let corner = (2.5 + 6.0 * open) * scale;
        // SAFETY: as above.
        unsafe {
            // The dot fades out by a third of the way open; the active one is a brighter bar.
            let dot = (1.0 - 3.0 * open).clamp(0.0, 1.0);
            if dot > 0.0 {
                let alpha = if active { 1.0 } else { 0.4 } * dot * reveal;
                let brush =
                    context.CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(alpha)), None)?;
                let half = (2.5 * scale).min((cell.bottom - cell.top) / 2.0);
                let middle = (cell.top + cell.bottom) / 2.0;
                let bar = D2D_RECT_F {
                    top: middle - half,
                    bottom: middle + half,
                    ..cell
                };
                context.FillRoundedRectangle(&rounded(bar, half), &brush);
            }
            if active && open > 0.0 {
                let wash = context.CreateSolidColorBrush(
                    &with_alpha(p.accent, p.ink_alpha(0.26 * open * reveal)),
                    None,
                )?;
                context.FillRoundedRectangle(&rounded(cell, corner), &wash);
            }
        }
        let shown = (open * 1.7 - 0.5).clamp(0.0, 1.0) * reveal;
        if shown > 0.0 {
            // 14 DIP glyphs grown from 40 % of that, centred in the button.
            let side = (14.0 * scale * (0.4 + 0.6 * open)).round();
            let (cx, cy) = (
                (cell.left + cell.right) / 2.0,
                (cell.top + cell.bottom) / 2.0,
            );
            let square = D2D_RECT_F {
                left: (cx - side / 2.0).round(),
                top: (cy - side / 2.0).round(),
                right: (cx - side / 2.0).round() + side,
                bottom: (cy - side / 2.0).round() + side,
            };
            let color = if active {
                with_alpha(p.accent, p.ink_alpha(shown))
            } else {
                with_alpha(p.ink, p.ink_alpha(0.7 * shown))
            };
            icon(context, tab, square, color)?;
        }
        if handle.focus == Some(position) {
            // SAFETY: as above.
            unsafe {
                let brush =
                    context.CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(reveal)), None)?;
                let gap = scale;
                let ring = D2D_RECT_F {
                    left: cell.left - gap,
                    top: cell.top - gap,
                    right: cell.right + gap,
                    bottom: cell.bottom + gap,
                };
                context.DrawRoundedRectangle(
                    &rounded(ring, corner + gap),
                    &brush,
                    2.0 * scale,
                    None,
                );
            }
        }
    }
    Ok(())
}

/// A page other than Media: its title, a small clock at the top right and its content, all at `reveal` so
/// they arrive as the pill's content leaves. The Timer page shows the soonest countdown big; the Shelf page
/// shows the shelf's slots.
pub(super) fn page(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    reveal: f32,
    p: &Palette,
) -> Result<()> {
    if reveal <= 0.0 {
        return Ok(());
    }
    let media = scene.track.is_some();
    let ink = |alpha: f32| with_alpha(p.ink, p.ink_alpha(alpha * reveal));
    let rect = |slot: Bounds| D2D_RECT_F {
        left: body.left + slot.x as f32,
        top: body.top + slot.y as f32,
        right: body.left + (slot.x + slot.width) as f32,
        bottom: body.top + (slot.y + slot.height) as f32,
    };
    let font = |family, size: f32, weight, designator, align| Font {
        family,
        size: size * scale,
        weight,
        designator,
        align,
    };
    let title = match scene.weather.filter(|_| scene.tab == Tab::Weather) {
        Some(view) => view.title.as_str(),
        None => scene.tab.name(),
    };
    draw_text(
        context,
        title,
        &font(
            TEXT_FONT,
            13.0,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            1.0,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        ),
        rect(layout::page_title(scene.dpi)),
        ink(1.0),
    )?;
    draw_text(
        context,
        scene.time,
        &font(
            TEXT_FONT,
            12.0,
            DWRITE_FONT_WEIGHT_NORMAL,
            TIME_DESIGNATOR,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
        ),
        rect(layout::page_clock(media, scene.dpi)),
        ink(0.6),
    )?;
    match scene.tab {
        Tab::Timer => match scene.timer {
            Some(view) => draw_text(
                context,
                &view.panel,
                &font(
                    DISPLAY_FONT,
                    26.0,
                    DWRITE_FONT_WEIGHT_BOLD,
                    1.0,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                ),
                rect(layout::page_value(scene.dpi)),
                ink(1.0),
            ),
            None => draw_text(
                context,
                "No timer running",
                &font(
                    TEXT_FONT,
                    12.0,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    1.0,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                ),
                rect(layout::page_value(scene.dpi)),
                ink(0.6),
            ),
        },
        Tab::Shelf => {
            shelf_row(context, scene, body, scale, p)?;
            if scene.shelf.first().is_some_and(|slot| slot.add) {
                draw_text(
                    context,
                    "Add files, or drop them here",
                    &font(
                        TEXT_FONT,
                        11.0,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        1.0,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                    ),
                    rect(layout::shelf_hint(scene.dpi)),
                    ink(0.6),
                )?;
            }
            Ok(())
        }
        Tab::Weather => weather_page(context, scene, body, scale, reveal, p),
        _ => Ok(()),
    }
}

/// The Weather page (design J "Hero"): a condition picture and the big temperature at the left with the day's
/// range and the condition beside them, the next hours as columns at the right, and the attribution as the
/// footer (the power notice takes its row while it shows). The columns are three in the plain panel and four
/// in the wider media one; with no data one line says why. Positions are DIPs from the body's corner.
fn weather_page(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    reveal: f32,
    p: &Palette,
) -> Result<()> {
    let Some(view) = scene.weather else {
        return Ok(());
    };
    let media = scene.track.is_some();
    let width = if media { 328.0 } else { 288.0 };
    let ink = |alpha: f32| with_alpha(p.ink, p.ink_alpha(alpha * reveal));
    let at = |x: f32, y: f32, w: f32, h: f32| D2D_RECT_F {
        left: body.left + x * scale,
        top: body.top + y * scale,
        right: body.left + (x + w) * scale,
        bottom: body.top + (y + h) * scale,
    };
    let font = |family, size: f32, weight, align| Font {
        family,
        size: size * scale,
        weight,
        designator: 1.0,
        align,
    };
    let (lead, middle) = (DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_CENTER);
    let normal = DWRITE_FONT_WEIGHT_NORMAL;
    if let Some(message) = view.message {
        let font = font(TEXT_FONT, 12.0, normal, lead);
        return draw_text(
            context,
            message,
            &font,
            at(16.0, 32.0, width, 16.0),
            ink(0.6),
        );
    }
    draw_art(
        context,
        weather_art(view.icon),
        at(16.0, 33.0, 28.0, 28.0),
        ink(1.0),
    )?;
    let temp = font(DISPLAY_FONT, 26.0, DWRITE_FONT_WEIGHT_BOLD, lead);
    draw_text(
        context,
        &view.temp,
        &temp,
        at(50.0, 30.0, 56.0, 32.0),
        ink(1.0),
    )?;
    let small = font(TEXT_FONT, 11.0, normal, lead);
    draw_text(
        context,
        &view.range,
        &small,
        at(108.0, 33.0, 72.0, 14.0),
        ink(0.8),
    )?;
    draw_text(
        context,
        view.cond,
        &small,
        at(108.0, 48.0, 72.0, 14.0),
        ink(0.6),
    )?;
    let columns = if media { 4 } else { 3 };
    for (index, cell) in view.hours.iter().take(columns).enumerate() {
        let x = 182.0 + 40.0 * index as f32;
        let label = font(TEXT_FONT, 10.0, normal, middle);
        draw_text(
            context,
            &cell.label,
            &label,
            at(x, 29.0, 40.0, 12.0),
            ink(0.6),
        )?;
        draw_art(
            context,
            weather_art(cell.icon),
            at(x + 15.0, 42.0, 10.0, 10.0),
            ink(0.85),
        )?;
        let value = font(TEXT_FONT, 12.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, middle);
        draw_text(
            context,
            &cell.temp,
            &value,
            at(x, 53.0, 40.0, 13.0),
            ink(1.0),
        )?;
    }
    if scene.notice.is_some() {
        return Ok(());
    }
    let row = if media { 98.0 } else { 66.0 };
    let footer = font(TEXT_FONT, 10.0, normal, lead);
    draw_text(
        context,
        &view.footer,
        &footer,
        at(16.0, row, width, 12.0),
        ink(0.45),
    )
}

/// Where the eye pair is centred in the pill, in DIPs from the body's left: the middle of the media pill's
/// title lane (design: 91 + 40), or left of the plain pill's centred clock.
fn eye_centre(media: bool) -> f32 {
    if media { 131.0 } else { 30.0 }
}

/// How visible the eyes are at an open `progress`: fully until the hover swell, gone as the body opens.
pub(super) fn eye_alpha(progress: f32) -> f32 {
    1.0 - motion::smooth((progress - 0.08) / 0.15)
}

/// The two capsules for the scene's mood and pose, at the pill's middle. Lids are flat cuts of the capsule
/// (a clip), not body-coloured overpaint, so they hold on every theme.
pub(super) fn draw_eyes(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    mood: eyes::Mood,
    pose: eyes::Pose,
    body: &D2D_RECT_F,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let alpha = eye_alpha(scene.progress);
    if alpha <= 0.0 {
        return Ok(());
    }
    let media = scene.track.is_some();
    // The close spring's undershoot briefly makes the pill shorter than 30: keep the row centred.
    let middle = (layout::notch_shape(scene.progress, media).height / 2.0).min(15.0);
    let Eye {
        w,
        h,
        dy,
        top,
        bottom,
    } = eyes::eye(mood, pose);
    let (w, h) = (w * scale, h * scale);
    let centre_y = body.top + (middle + dy) * scale;
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw and
    // every clip pushed below is popped in the same iteration.
    unsafe {
        let brush = context.CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(alpha)), None)?;
        for side in [-1.0f32, 1.0] {
            let centre_x = body.left + eye_centre(media) * scale + side * eyes::GAP / 2.0 * scale;
            let capsule = D2D_RECT_F {
                left: centre_x - w / 2.0,
                top: centre_y - h / 2.0,
                right: centre_x + w / 2.0,
                bottom: centre_y + h / 2.0,
            };
            let lids = D2D_RECT_F {
                top: capsule.top + top * h,
                bottom: capsule.bottom - bottom * h,
                ..capsule
            };
            if lids.bottom <= lids.top {
                continue;
            }
            context.PushAxisAlignedClip(&lids, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            context.FillRoundedRectangle(&rounded(capsule, w.min(h) / 2.0), &brush);
            context.PopAxisAlignedClip();
        }
    }
    Ok(())
}
