//! The dock moon's popup: its hero (the moon, name, lit share and Hijri date) and its date rows.

use super::*;

/// The moon popup's hero in `cell` (the hero's 92 DIPs and the divider under it): the moon at 64 on its `tile`
/// disc, and the name, lit share and Hijri date (`text`, one per line) beside it.
pub(super) fn hero(
    context: &ID2D1DeviceContext,
    cell: D2D_RECT_F,
    lit: f32,
    right: bool,
    text: &str,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let light = p.ink.0 < 128;
    let crate::theme::Rgba(r, g, b, a) = p.tile;
    // The disc is 18 of the glyph's 24 units: a 64 DIP moon sits in an 85.3 DIP square.
    let side = 64.0 * scale * 24.0 / 18.0;
    let (x, y) = (
        cell.left + 8.0 * scale + 32.0 * scale,
        cell.top + 46.0 * scale,
    );
    let ink = glyphs::MoonInk {
        lit: if light {
            rgba(255, 255, 255, 1.0)
        } else {
            with_alpha(p.ink, 1.0)
        },
        disc: Some(rgba(r, g, b, a)),
        ring: light.then(|| (tint(p.ink_dim(false, 0.22)), 1.75 * scale)),
    };
    let square = D2D_RECT_F {
        left: x - side / 2.0,
        top: y - side / 2.0,
        right: x + side / 2.0,
        bottom: y + side / 2.0,
    };
    glyphs::moon(context, square, lit, right, &ink)?;
    let left = cell.left + 86.0 * scale;
    let at = |top: f32, height: f32| D2D_RECT_F {
        left,
        top: cell.top + top * scale,
        right: cell.right - 8.0 * scale,
        bottom: cell.top + (top + height) * scale,
    };
    let font = |family, size: f32, weight| Font {
        family,
        size: size * scale,
        weight,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_LEADING,
    };
    for (line, (top, height, font, alpha)) in text.lines().zip([
        (
            17.0,
            21.0,
            font(DISPLAY_FONT, 15.0, DWRITE_FONT_WEIGHT_SEMI_BOLD),
            1.0,
        ),
        (
            41.0,
            16.0,
            font(TEXT_FONT, 12.0, DWRITE_FONT_WEIGHT_NORMAL),
            0.7,
        ),
        (
            60.0,
            14.0,
            font(TEXT_FONT, 10.5, DWRITE_FONT_WEIGHT(500)),
            0.55,
        ),
    ]) {
        draw_text(
            context,
            line,
            &font,
            at(top, height),
            tint(p.ink_dim(false, alpha)),
        )?;
    }
    let divider = D2D_RECT_F {
        left: cell.left + 8.0 * scale,
        top: cell.top + 92.0 * scale,
        right: cell.right - 8.0 * scale,
        bottom: cell.top + 94.0 * scale,
    };
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw.
    unsafe {
        let brush = context.CreateSolidColorBrush(&tint(p.ink_dim(false, 0.08)), None)?;
        context.FillRoundedRectangle(&rounded(divider, scale), &brush);
    }
    Ok(())
}

/// A moon date row: the label in ink, its date fainter on the right; neither is pressed.
pub(super) fn date_row(
    context: &ID2D1DeviceContext,
    cell: D2D_RECT_F,
    label: &str,
    date: &str,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let font = Font {
        family: TEXT_FONT,
        size: 12.0 * scale,
        weight: DWRITE_FONT_WEIGHT_NORMAL,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_LEADING,
    };
    let inner = D2D_RECT_F {
        left: cell.left + 8.0 * scale,
        right: cell.right - 8.0 * scale,
        ..cell
    };
    draw_text(context, label, &font, inner, tint(p.ink_dim(false, 0.86)))?;
    let trailing = Font {
        align: DWRITE_TEXT_ALIGNMENT_TRAILING,
        ..font
    };
    draw_text(
        context,
        date,
        &trailing,
        inner,
        tint(p.ink_dim(false, 0.55)),
    )
}
