use super::*;
use windows::Win32::Graphics::{
    Direct2D::{D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1, ID2D1Geometry},
    DirectWrite::{DWRITE_LINE_SPACING_METHOD_UNIFORM, DWRITE_WORD_WRAPPING_WRAP},
};

pub(super) fn shadow_strength(scene: &NotchScene) -> f32 {
    let open = motion::smooth(scene.progress);
    if scene.extra.footer() {
        open.max(0.55 * scene.fx.notice)
    } else {
        open
    }
}

pub(super) fn glance_body(scene: &NotchScene, body: D2D_RECT_F, scale: f32) -> D2D_RECT_F {
    if !scene.extra.footer() || motion::reveal(scene.progress) > 0.0 {
        return body;
    }
    let shape = layout::notch_shape_with_notice(scene.progress, scene.track.is_some(), false);
    let offset = (body.right - body.left - (shape.width * scale).round()) / 2.0;
    D2D_RECT_F {
        left: body.left + offset.floor(),
        right: body.right - offset.ceil(),
        bottom: body.top + (shape.height * scale).round(),
        ..body
    }
}

pub(super) fn draw(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    outline: &ID2D1PathGeometry,
    p: &Palette,
) -> Result<()> {
    let (Some(footer), Some(text)) = (scene.footer, scene.notice) else {
        return Ok(());
    };
    let scale = scene.dpi as f32 / 96.0;
    let rect = |slot: Bounds| D2D_RECT_F {
        left: body.left + slot.x as f32,
        top: body.top + slot.y as f32,
        right: body.left + (slot.x + slot.width) as f32,
        bottom: body.top + (slot.y + slot.height) as f32,
    };
    let plate = rect(footer.plate);
    let mut copy = rect(footer.text);
    let alpha = scene.fx.notice
        * if footer.compact {
            1.0
        } else {
            motion::reveal(scene.progress)
        };
    let mut mask = D2D1_LAYER_PARAMETERS1 {
        contentBounds: plate,
        geometricMask: ManuallyDrop::new(Some(outline.cast::<ID2D1Geometry>()?)),
        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
        maskTransform: Matrix3x2::identity(),
        opacity: alpha,
        opacityBrush: ManuallyDrop::new(None),
        layerOptions: D2D1_LAYER_OPTIONS1_NONE,
    };
    // SAFETY: the live context and the referenced outline outlive the layer. Pop on all draw results.
    unsafe { context.PushLayer(&mask, None) };
    let painted = (|| {
        let fill = |rect: D2D_RECT_F, radius, color| -> Result<()> {
            // SAFETY: the brush and the draw are local to this live layer.
            unsafe {
                let brush = context.CreateSolidColorBrush(&color, None)?;
                context.FillRoundedRectangle(&rounded(rect, radius), &brush);
            }
            Ok(())
        };
        let ink = |alpha| with_alpha(p.ink, p.ink_alpha(alpha));
        fill(plate, footer.plate_radius(), with_alpha(p.ink, 0.075))?;
        let middle = (plate.top + plate.bottom) / 2.0;
        let glyph = if scene.extra == Extra::Bell {
            Glyph::Bell
        } else {
            Glyph::Clock
        };
        glyphs::draw(
            context,
            pixel_square(plate.left + 9.0 * scale, middle - 9.0 * scale, 18.0 * scale),
            glyph,
            ink(0.82),
        )?;
        let font = Font {
            family: TEXT_FONT,
            size: 12.0 * scale,
            weight: DWRITE_FONT_WEIGHT_NORMAL,
            designator: 1.0,
            align: DWRITE_TEXT_ALIGNMENT_LEADING,
        };
        if scene.extra == Extra::Bell {
            if copy.right <= copy.left {
                return Ok(());
            }
            return bell_text(context, text, font, copy, scale, footer.compact, ink(1.0));
        }
        if scene.extra == Extra::Snoozed {
            return if copy.right > copy.left {
                draw_text(context, text, &font, copy, ink(1.0))
            } else {
                Ok(())
            };
        }
        let button = rect(footer.button);
        copy.right = body.left + footer.label_right() as f32;
        let heading = Font {
            weight: DWRITE_FONT_WEIGHT_SEMI_BOLD,
            ..font
        };
        let title = if text.is_empty() {
            copy
        } else {
            D2D_RECT_F {
                bottom: middle,
                ..copy
            }
        };
        if copy.right > copy.left {
            draw_text(context, "Reminder", &heading, title, ink(1.0))?;
            if !text.is_empty() {
                draw_text(
                    context,
                    text,
                    &font,
                    D2D_RECT_F {
                        top: middle,
                        ..copy
                    },
                    ink(0.82),
                )?;
            }
        }
        let neutral = !footer.compact
            && match scene.tab {
                Tab::Media => scene.track.is_some(),
                Tab::Timer => scene.timer.is_some_and(|timer| !timer.done),
                _ => false,
            };
        let (back, front) = if neutral {
            (with_alpha(p.ink, 0.15), ink(1.0))
        } else {
            (
                with_alpha(p.accent, 1.0),
                with_alpha(p.ink_on_accent(), 1.0),
            )
        };
        fill(button, (button.bottom - button.top) / 2.0, back)?;
        let action = Font {
            align: DWRITE_TEXT_ALIGNMENT_CENTER,
            ..heading
        };
        draw_text(context, "Snooze 10 min", &action, button, front)
    })();
    // SAFETY: balances the layer above and releases the owned geometry once it is no longer in use.
    unsafe {
        context.PopLayer();
        ManuallyDrop::drop(&mut mask.geometricMask);
    }
    painted
}

fn bell_text(
    context: &ID2D1DeviceContext,
    text: &str,
    mut font: Font,
    rect: D2D_RECT_F,
    scale: f32,
    compact: bool,
    ink: D2D1_COLOR_F,
) -> Result<()> {
    font.size = 13.0 * scale;
    let step = if compact { 14.0 } else { 16.0 } * scale;
    let layout = text_layout(text, &font, rect.right - rect.left, 2.0 * step)?;
    // SAFETY: the layout is local. DirectWrite wraps and trims at shaped cluster boundaries.
    unsafe {
        layout.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
        layout.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, step, 0.8 * step)?;
    }
    let top = (rect.top + rect.bottom) / 2.0 - step;
    draw_layout(context, &layout, D2D_RECT_F { top, ..rect }, ink)
}
