//! Direct2D drawing for the notch tabs and the mascot (features tabs-eyes): the hanging handle with its five
//! icons, the pages other than Media, the opacity layer that fades the pill out as a page fades in, and the
//! squash-capsule eyes. The icons are `glyphs.rs`, drawn in the theme and accent colours. Nothing here runs unless the host puts a rail, a page or eyes in the scene.

use super::*;
use crate::{
    eyes::{self, Eye},
    layout::NotchBody,
    tabs::Tab,
};
use windows::Win32::Graphics::Direct2D::{D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1};

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

/// The hanging handle (design T3), drawn after the body clip is popped because it lies below the body: a
/// body-coloured shape with the notch's own concave ears, then per tab a dot that becomes an icon as the
/// handle opens (`p`) and the active one accented.
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

/// The handle's dots, icons and active wash.
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
                    &with_alpha(p.accent, p.ink_alpha(0.22 * open * reveal)),
                    None,
                )?;
                context.FillRoundedRectangle(&rounded(cell, corner), &wash);
            }
        }
        let shown = (open * 1.7 - 0.5).clamp(0.0, 1.0) * reveal;
        if shown > 0.0 {
            // 16 DIP glyphs grown from 40 % of that, centred in the button.
            let side = (16.0 * scale * (0.4 + 0.6 * open)).round();
            let (cx, cy) = (
                if active {
                    cell.left + (cell.bottom - cell.top) / 2.0
                } else {
                    (cell.left + cell.right) / 2.0
                },
                (cell.top + cell.bottom) / 2.0,
            );
            let square = D2D_RECT_F {
                left: (cx - side / 2.0).round(),
                top: (cy - side / 2.0).round(),
                right: (cx - side / 2.0).round() + side,
                bottom: (cy - side / 2.0).round() + side,
            };
            let color = if active {
                with_alpha(glyphs::active_ink(p), p.ink_alpha(shown))
            } else {
                with_alpha(p.ink, p.ink_alpha(0.7 * shown))
            };
            glyphs::draw(context, square, Glyph::tab(tab), color)?;
            if active {
                // The active tab names its page beside its icon.
                let name = Font {
                    family: TEXT_FONT,
                    size: 10.0 * scale,
                    weight: DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    designator: 1.0,
                    align: DWRITE_TEXT_ALIGNMENT_LEADING,
                };
                let text = D2D_RECT_F {
                    left: cell.left + 28.0 * scale,
                    ..cell
                };
                let alpha = (open * 3.0 - 2.0).clamp(0.0, 1.0) * reveal;
                draw_text(
                    context,
                    tab.name(),
                    &name,
                    text,
                    with_alpha(glyphs::active_ink(p), p.ink_alpha(alpha)),
                )?;
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
    // A tab switch slides the page in from the side it came from: the title and clock first, the content a
    // beat later (the stagger).
    let slide = |lag: f32| {
        let t = ((scene.fx.page - lag) / (1.0 - lag)).clamp(0.0, 1.0);
        let offset = scene.fx.dir * 18.0 * scale * (1.0 - motion::smooth(t));
        // SAFETY: a transform on the frame's live context; the notch resets it after the page.
        unsafe { context.SetTransform(&Matrix3x2::translation(offset, 0.0)) };
    };
    slide(0.0);
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
    slide(0.25);
    match scene.tab {
        Tab::Timer => match scene.timer.filter(|view| !view.done) {
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
                if view.done {
                    with_alpha(p.accent, p.ink_alpha(reveal))
                } else {
                    ink(1.0)
                },
            ),
            None => {
                // Idle, or the last one just finished: the preset buttons start the next.
                let line = scene
                    .timer
                    .map_or("Start a timer", |view| view.panel.as_str());
                let mut slot = layout::page_value(scene.dpi);
                slot.height /= 2;
                draw_text(
                    context,
                    line,
                    &font(
                        TEXT_FONT,
                        12.0,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                        1.0,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                    ),
                    rect(slot),
                    with_alpha(p.accent, p.ink_alpha(reveal)),
                )?;
                for (index, minutes) in crate::timer::PRESETS.into_iter().enumerate() {
                    pill(
                        context,
                        scene,
                        body,
                        index,
                        &format!("{minutes} min"),
                        reveal,
                        p,
                    )?;
                }
                Ok(())
            }
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
        Tab::Prayer => prayer_page(context, scene, body, scale, reveal, p),
        Tab::Limits => limits_page(context, scene, body, scale, reveal, p),
        _ => Ok(()),
    }
}

/// A page button (`layout::page_button`): an outlined pill with its label in the accent colour.
fn pill(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    index: usize,
    label: &str,
    reveal: f32,
    p: &Palette,
) -> Result<()> {
    let scale = scene.dpi as f32 / 96.0;
    let slot = layout::page_button(index, scene.dpi);
    let square = D2D_RECT_F {
        left: body.left + slot.x as f32,
        top: body.top + slot.y as f32,
        right: body.left + (slot.x + slot.width) as f32,
        bottom: body.top + (slot.y + slot.height) as f32,
    };
    // SAFETY: a brush and an outline on the frame's own device context.
    unsafe {
        let brush = context
            .CreateSolidColorBrush(&with_alpha(p.accent, p.ink_alpha(0.6 * reveal)), None)?;
        context.DrawRoundedRectangle(&rounded(square, 6.0 * scale), &brush, scale, None);
    }
    let font = Font {
        family: TEXT_FONT,
        size: 11.0 * scale,
        weight: DWRITE_FONT_WEIGHT_SEMI_BOLD,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_CENTER,
    };
    draw_text(
        context,
        label,
        &font,
        square,
        with_alpha(p.accent, p.ink_alpha(reveal)),
    )
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
        draw_text(
            context,
            message,
            &font,
            at(16.0, 32.0, width, 16.0),
            ink(0.6),
        )?;
        return pill(context, scene, body, 0, "Set city", reveal, p);
    }
    glyphs::draw(
        context,
        at(16.0, 33.0, 28.0, 28.0),
        Glyph::weather(view.icon),
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
        glyphs::draw(
            context,
            at(x + 14.0, 41.0, 12.0, 12.0),
            Glyph::weather(cell.icon),
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

/// The Prayer page: the next prayer and its countdown at the left, the five times as columns at the right with
/// the current one on a wash, and the Hijri date as the footer (the notice takes its row while it shows).
fn prayer_page(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    reveal: f32,
    p: &Palette,
) -> Result<()> {
    let Some(view) = scene.prayer else {
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
        draw_text(
            context,
            message,
            &font(TEXT_FONT, 12.0, normal, lead),
            at(16.0, 32.0, width, 16.0),
            ink(0.6),
        )?;
        return pill(context, scene, body, 0, "Set city", reveal, p);
    }
    // Wide enough for "Maghrib" at the name size, so no name is cut.
    let column = if media { 44.0 } else { 40.0 };
    let left = 16.0 + width - column * view.rows.len() as f32;
    let name = font(DISPLAY_FONT, 18.0, DWRITE_FONT_WEIGHT_BOLD, lead);
    draw_text(
        context,
        &view.next,
        &name,
        at(16.0, 29.0, left - 20.0, 24.0),
        ink(1.0),
    )?;
    let small = font(TEXT_FONT, 11.0, normal, lead);
    draw_text(
        context,
        &view.left,
        &small,
        at(16.0, 54.0, left - 20.0, 14.0),
        ink(0.7),
    )?;
    for (index, row) in view.rows.iter().enumerate() {
        let x = left + column * index as f32;
        // The wash marks the prayer the headline names (the next); the current one is only brighter and bolder.
        if row.name == view.next {
            // SAFETY: a brush and a fill on the frame's own device context.
            unsafe {
                let wash = context.CreateSolidColorBrush(
                    &with_alpha(p.accent, p.ink_alpha(0.22 * reveal)),
                    None,
                )?;
                context.FillRoundedRectangle(
                    &rounded(at(x + 1.0, 27.0, column - 2.0, 29.0), 6.0 * scale),
                    &wash,
                );
            }
        }
        let weight = if row.current {
            DWRITE_FONT_WEIGHT_SEMI_BOLD
        } else {
            normal
        };
        let alpha = if row.current { 1.0 } else { 0.6 };
        draw_text(
            context,
            row.name,
            &font(TEXT_FONT, 9.5, normal, middle),
            at(x, 29.0, column, 12.0),
            ink(alpha),
        )?;
        draw_text(
            context,
            &row.time,
            &font(TEXT_FONT, 11.0, weight, middle),
            at(x, 41.0, column, 14.0),
            ink(if row.current { 1.0 } else { 0.85 }),
        )?;
    }
    if scene.notice.is_some() || view.hijri.is_empty() {
        return Ok(());
    }
    let row = if media { 98.0 } else { 66.0 };
    draw_text(
        context,
        &view.hijri,
        &font(TEXT_FONT, 10.0, normal, lead),
        at(16.0, row, width, 12.0),
        ink(0.5),
    )
}

/// The Limits page: a Refresh button beside the title and one line per tool: name, then for each window a thin
/// bar, `NN%` and its reset time, then "as of" (or the age when stale). A window that has reset says so, a
/// hidden one draws nothing, and a tool without a reading says why. Stale rows are dimmed and say so in text.
fn limits_page(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    reveal: f32,
    p: &Palette,
) -> Result<()> {
    use crate::limits::Line;
    let Some(view) = scene.limits else {
        return Ok(());
    };
    let ink = |alpha: f32| with_alpha(p.ink, p.ink_alpha(alpha * reveal));
    let at = |x: f32, y: f32, w: f32, h: f32| D2D_RECT_F {
        left: body.left + x * scale,
        top: body.top + y * scale,
        right: body.left + (x + w) * scale,
        bottom: body.top + (y + h) * scale,
    };
    let font = |size: f32, weight, align| Font {
        family: TEXT_FONT,
        size: size * scale,
        weight,
        designator: 1.0,
        align,
    };
    let (lead, trail) = (
        DWRITE_TEXT_ALIGNMENT_LEADING,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
    );
    let normal = DWRITE_FONT_WEIGHT_NORMAL;
    // The Refresh button: an outlined label.
    let button = layout::refresh(scene.dpi);
    let square = D2D_RECT_F {
        left: body.left + button.x as f32,
        top: body.top + button.y as f32,
        right: body.left + (button.x + button.width) as f32,
        bottom: body.top + (button.y + button.height) as f32,
    };
    // SAFETY: a brush and an outline on the frame's own device context.
    unsafe {
        let brush = context.CreateSolidColorBrush(&ink(0.3), None)?;
        context.DrawRoundedRectangle(&rounded(square, 6.0 * scale), &brush, scale, None);
    }
    draw_text(
        context,
        if view.refreshing {
            "Refreshing..."
        } else {
            "Refresh"
        },
        &font(10.0, normal, DWRITE_TEXT_ALIGNMENT_CENTER),
        square,
        ink(0.8),
    )?;
    let accent = |alpha: f32| with_alpha(p.accent, p.ink_alpha(alpha * reveal));
    for (label, x) in [("5h used", 72.0), ("Weekly used", 162.0)] {
        draw_text(
            context,
            label,
            &font(9.5, normal, lead),
            at(x, 26.0, 86.0, 12.0),
            ink(0.6),
        )?;
    }
    for (index, row) in view.rows.iter().enumerate() {
        let y = 38.0 + 17.0 * index as f32;
        let dim = if row.stale { 0.5 } else { 1.0 };
        draw_text(
            context,
            row.name,
            &font(10.5, DWRITE_FONT_WEIGHT_SEMI_BOLD, lead),
            at(16.0, y, 54.0, 16.0),
            ink(dim),
        )?;
        if let Some(why) = row.why {
            draw_text(
                context,
                why,
                &font(10.5, normal, lead),
                at(72.0, y, 230.0, 16.0),
                ink(0.6),
            )?;
            continue;
        }
        // Each window: bar (20), percent (24), reset time; the weekly one has the day too.
        for (line, x, reset_width) in [(&row.h5, 72.0, 36.0), (&row.wk, 162.0, 52.0)] {
            let (Line::Used { reset, .. } | Line::Reset { reset }) = line else {
                continue;
            };
            let after = x + 51.0;
            match line {
                Line::Used { pct, .. } => {
                    // SAFETY: brushes and fills on the frame's own device context.
                    unsafe {
                        let track = context.CreateSolidColorBrush(&ink(0.18 * dim), None)?;
                        let fill = context.CreateSolidColorBrush(&accent(dim), None)?;
                        let bar = at(x, y + 6.0, 22.0, 4.0);
                        context.FillRoundedRectangle(&rounded(bar, 2.0 * scale), &track);
                        let mut part = bar;
                        part.right = bar.left + (bar.right - bar.left) * f32::from(*pct) / 100.0;
                        context.FillRoundedRectangle(&rounded(part, 2.0 * scale), &fill);
                    }
                    draw_text(
                        context,
                        &format!("{pct}%"),
                        &font(10.5, normal, trail),
                        at(x + 23.0, y, 28.0, 16.0),
                        ink(0.9 * dim),
                    )?;
                }
                Line::Reset { .. } => draw_text(
                    context,
                    "reset",
                    &font(10.5, normal, lead),
                    at(x, y, 50.0, 16.0),
                    ink(0.7 * dim),
                )?,
                Line::Hidden => {}
            }
            draw_text(
                context,
                reset,
                &font(10.0, normal, lead),
                at(after, y, reset_width, 16.0),
                ink(0.55 * dim),
            )?;
        }
        draw_text(
            context,
            &row.as_of,
            &font(8.5, normal, trail),
            at(256.0, y, 52.0, 16.0),
            ink(0.45),
        )?;
    }
    Ok(())
}

/// Where the eye pair is centred in the pill, in DIPs from the body's left: the middle of the media pill's
/// title lane (design: 91 + 40), or the plain pill's middle, where the clock fades out for them.
fn eye_centre(media: bool, body_width: f32) -> f32 {
    if media { 131.0 } else { body_width / 2.0 }
}

/// How visible the eyes are at an open `progress`: fully until the hover swell, gone as the body opens.
pub(super) fn eye_alpha(progress: f32) -> f32 {
    1.0 - motion::smooth((progress - 0.08) / 0.15)
}

/// How much of the pill's time (plain) or title (media) the eyes have replaced right now, 0 to 1.
pub(super) fn eyes_shown(scene: &NotchScene) -> f32 {
    scene
        .eyes
        .map_or(0.0, |frame| frame.alpha * eye_alpha(scene.progress))
}

/// The two capsules for the scene's mood and pose, at the pill's middle. Lids are flat cuts of the capsule
/// (a clip), not body-coloured overpaint, so they hold on every theme.
pub(super) fn draw_eyes(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    frame: eyes::Frame,
    body: &D2D_RECT_F,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let alpha = eyes_shown(scene);
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
    } = eyes::eye(frame.mood, frame.pose);
    let (w, h) = (w * scale, h * scale);
    let centre_y = body.top + (middle + dy) * scale;
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw and
    // every clip pushed below is popped in the same iteration.
    unsafe {
        let brush = context.CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(alpha)), None)?;
        for side in [-1.0f32, 1.0] {
            let middle_x = body.left + eye_centre(media, (body.right - body.left) / scale) * scale;
            let centre_x = middle_x + (side * eyes::GAP / 2.0 + frame.look) * scale;
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

/// The privacy dots (features B5): 6 DIP circles inset 14 from the body's right edge, on the pill's middle line,
/// the camera one nearest the edge. Only a dot with some opacity is drawn.
pub(super) fn draw_privacy(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
) -> Result<()> {
    let middle =
        (layout::notch_shape(scene.progress, scene.track.is_some()).height / 2.0).min(15.0);
    let (r, y) = (3.0 * scale, body.top + middle * scale);
    // Orange for the microphone, green for the camera: fixed signal colours on every theme.
    for (slot, (alpha, colour)) in [
        (scene.privacy[1], (34, 197, 94)),
        (scene.privacy[0], (245, 158, 11)),
    ]
    .into_iter()
    .enumerate()
    {
        if alpha <= 0.0 {
            continue;
        }
        let x = body.right - (14.0 + 9.0 * slot as f32) * scale;
        let dot = D2D_RECT_F {
            left: x - r,
            top: y - r,
            right: x + r,
            bottom: y + r,
        };
        // SAFETY: context is live between surface BeginDraw/EndDraw; the brush lives for this draw.
        unsafe {
            let brush = context.CreateSolidColorBrush(&with_alpha(colour, alpha), None)?;
            context.FillRoundedRectangle(&rounded(dot, r), &brush);
        }
    }
    Ok(())
}
