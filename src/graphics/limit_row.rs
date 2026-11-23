//! An AI limit row (`cook/design/limits-icons-polish.md` A): the label and the used percent over a thin
//! capsule bar. The fill is a capsule of the track's own height, so it never leaves the track.

use super::*;
use windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_MEDIUM;

/// Paints one limit row in `cell` (a 34 DIP list row): `label` leading, `percent` trailing, the bar under them.
pub(super) fn paint(
    context: &ID2D1DeviceContext,
    p: &Palette,
    cell: D2D_RECT_F,
    label: &str,
    percent: u8,
    scale: f32,
) -> Result<()> {
    let (left, right) = (
        (cell.left + 12.0 * scale).round(),
        (cell.right - 12.0 * scale).round(),
    );
    let top = cell.top + 4.0 * scale;
    let digits = D2D_RECT_F {
        left: right - 40.0 * scale,
        top,
        right,
        bottom: top + 18.0 * scale,
    };
    let font = |weight, align| Font {
        family: TEXT_FONT,
        size: 13.0 * scale,
        weight,
        designator: 1.0,
        align,
    };
    draw_text(
        context,
        label,
        &font(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING),
        D2D_RECT_F {
            left,
            right: digits.left - 8.0 * scale,
            ..digits
        },
        tint(p.ink_dim(false, 0.70)),
    )?;
    // Segoe UI Variable's default figures are tabular, so the percents line up.
    draw_text(
        context,
        &format!("{percent}%"),
        &font(DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_TEXT_ALIGNMENT_TRAILING),
        digits,
        tint(p.ink_dim(false, 1.0)),
    )?;
    let height = (4.0 * scale).round();
    let bar_top = (cell.top + 23.0 * scale).round();
    let track = D2D_RECT_F {
        left,
        top: bar_top,
        right,
        bottom: bar_top + height,
    };
    draw_track(context, p, &rounded(track, height / 2.0), p.ink_alpha(0.18))?;
    if percent == 0 {
        return Ok(());
    }
    let span = right - left;
    let width = (span * f32::from(percent.min(100)) / 100.0)
        .round()
        .clamp(height, span);
    let fill = D2D_RECT_F {
        right: left + width,
        ..track
    };
    // SAFETY: live draw context between BeginDraw/EndDraw; the brush is scoped to this draw.
    unsafe {
        let brush = context.CreateSolidColorBrush(&tint(p.accent_dim(false, 1.0)), None)?;
        context.FillRoundedRectangle(&rounded(fill, height / 2.0), &brush);
    }
    Ok(())
}
