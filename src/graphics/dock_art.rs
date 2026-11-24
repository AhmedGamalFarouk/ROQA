use super::*;

pub(super) fn quick_tile(
    context: &ID2D1DeviceContext,
    cell: D2D_RECT_F,
    text: &str,
    (action, on, details): (crate::dock_menu::Quick, bool, bool),
    hover: bool,
    scale: f32,
    palette: &Palette,
) -> Result<()> {
    use crate::dock_menu::Quick;
    let (name, state) = text.split_once('\t').unwrap_or((text, ""));
    let middle = (cell.top + cell.bottom) / 2.0;
    let color = tint(palette.ink_dim(false, if on { 1.0 } else { 0.72 }));
    let glyph = match action {
        Quick::WiFi => Glyph::WiFi,
        Quick::Bluetooth => Glyph::Bluetooth,
        Quick::Airplane => Glyph::Airplane,
        Quick::Hotspot => Glyph::Hotspot,
        Quick::KeepAwake => Glyph::Sun,
        _ => Glyph::MoonTheme,
    };
    // SAFETY: the live Direct2D context copies these brush and capsule values.
    unsafe {
        let fill = context.CreateSolidColorBrush(
            &tint(palette.ink_dim(
                false,
                if hover {
                    0.22
                } else if on {
                    0.14
                } else {
                    0.07
                },
            )),
            None,
        )?;
        context.FillRoundedRectangle(&rounded(cell, (cell.bottom - cell.top) / 2.0), &fill);
    }
    glyphs::draw(
        context,
        pixel_square(
            cell.left + 10.0 * scale,
            middle - 10.0 * scale,
            20.0 * scale,
        ),
        glyph,
        color,
    )?;
    let right = cell.right
        - if on || details {
            33.0 * scale
        } else {
            8.0 * scale
        };
    let font = Font {
        family: TEXT_FONT,
        size: 13.0 * scale,
        weight: DWRITE_FONT_WEIGHT_NORMAL,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_LEADING,
    };
    let left = cell.left + 36.0 * scale;
    draw_text(
        context,
        name,
        &font,
        D2D_RECT_F {
            left,
            right,
            top: middle - 17.0 * scale,
            bottom: middle + scale,
        },
        color,
    )?;
    draw_text(
        context,
        state,
        &Font {
            size: 11.0 * scale,
            ..font
        },
        D2D_RECT_F {
            left,
            right,
            top: middle,
            bottom: middle + 16.0 * scale,
        },
        tint(palette.ink_dim(false, 0.70)),
    )?;
    if on {
        let dot = pixel_square(
            cell.right - (if details { 35.0 } else { 14.0 }) * scale,
            middle - 3.0 * scale,
            6.0 * scale,
        );
        // SAFETY: the live Direct2D context copies the brush and dot geometry.
        unsafe {
            let lamp =
                context.CreateSolidColorBrush(&tint(palette.accent_dim(false, 1.0)), None)?;
            context.FillRoundedRectangle(&rounded(dot, 3.0 * scale), &lamp);
        }
    }
    if details {
        glyphs::draw(
            context,
            pixel_square(
                cell.right - 23.0 * scale,
                middle - 7.0 * scale,
                14.0 * scale,
            ),
            Glyph::Forward,
            color,
        )?;
    }
    Ok(())
}

pub(super) fn calendar_week(
    context: &ID2D1DeviceContext,
    cell: D2D_RECT_F,
    text: &str,
    today: Option<u8>,
    font: &Font,
    palette: &Palette,
) -> Result<()> {
    let width = (cell.right - cell.left) / 7.0;
    let centered = Font {
        align: DWRITE_TEXT_ALIGNMENT_CENTER,
        ..*font
    };
    for (column, day) in text.split('\t').enumerate() {
        let left = cell.left + width * column as f32;
        let color = if today == Some(column as u8) {
            tint(palette.accent_dim(false, 1.0))
        } else {
            tint(palette.ink_dim(false, 0.9))
        };
        draw_text(
            context,
            day,
            &centered,
            D2D_RECT_F {
                left,
                right: left + width,
                ..cell
            },
            color,
        )?;
    }
    Ok(())
}
