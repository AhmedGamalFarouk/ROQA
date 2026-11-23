//! The screenshot pill's chip on the notice row (design `cook/design/moon-and-screenshot-pill.md`, part 2):
//! a `tile` capsule with the newest thumbnail, the next-older one peeking behind it, and two lines.

use super::{
    DWRITE_TEXT_ALIGNMENT_LEADING, Font, ID2D1Bitmap1, ID2D1DeviceContext, IconPixels, NotchScene,
    Palette, Result, TEXT_FONT, cached, draw_text, layout, motion, rgba, rounded, text_width,
    upload, with_alpha,
};
use crate::{shots, theme::Rgba};
use std::cell::RefCell;
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_MEDIUM;

/// What the chip shows for one frame.
pub struct ShotScene<'a> {
    pub top: (u64, &'a IconPixels),
    /// The next-older shot, peeking behind.
    pub under: Option<(u64, &'a IconPixels)>,
    pub count: usize,
    /// 0 to 1: the thumbnail lands (pop, scale .92 to 1) and the text fades in.
    pub land: f32,
    /// 0 to 1: the pointer is on the chip.
    pub hover: f32,
    pub dragging: bool,
}

/// The two thumbnails' device bitmaps, uploaded once per shot.
pub type Thumbs = RefCell<[Option<(u64, ID2D1Bitmap1)>; 2]>;

fn fill(
    context: &ID2D1DeviceContext,
    rect: D2D_RECT_F,
    radius: f32,
    colour: windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F,
) -> Result<()> {
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw.
    unsafe {
        let brush = context.CreateSolidColorBrush(&colour, None)?;
        context.FillRoundedRectangle(&rounded(rect, radius), &brush);
    }
    Ok(())
}

/// A thumbnail filling `rect` (it is already 16:9) as a capsule at `opacity`.
fn thumb(
    context: &ID2D1DeviceContext,
    rect: D2D_RECT_F,
    radius: f32,
    bitmap: &ID2D1Bitmap1,
    width: u32,
    opacity: f32,
) -> Result<()> {
    use windows::Win32::Graphics::Direct2D::{
        D2D1_BITMAP_BRUSH_PROPERTIES1, D2D1_BRUSH_PROPERTIES, D2D1_EXTEND_MODE_CLAMP,
        D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, D2D1_INTERPOLATION_MODE_LINEAR,
    };
    let scale = (rect.right - rect.left) / width as f32;
    let properties = D2D1_BITMAP_BRUSH_PROPERTIES1 {
        extendModeX: D2D1_EXTEND_MODE_CLAMP,
        extendModeY: D2D1_EXTEND_MODE_CLAMP,
        interpolationMode: D2D1_INTERPOLATION_MODE_LINEAR,
    };
    let placement = D2D1_BRUSH_PROPERTIES {
        opacity,
        transform: windows_numerics::Matrix3x2 {
            M11: scale,
            M12: 0.0,
            M21: 0.0,
            M22: scale,
            M31: rect.left,
            M32: rect.top,
        },
    };
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw, and the
    // bitmap belongs to the same device.
    unsafe {
        let brush = context.CreateBitmapBrush(bitmap, Some(&properties), Some(&placement))?;
        brush.SetInterpolationMode1(D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC);
        context.FillRoundedRectangle(&rounded(rect, radius), &brush);
    }
    Ok(())
}

/// A kept screenshot on a Shelf capsule: a 16:9 thumbnail capsule centred on the file glyph's `square`.
pub fn small(
    context: &ID2D1DeviceContext,
    square: D2D_RECT_F,
    scale: f32,
    pixels: &IconPixels,
    opacity: f32,
) -> Result<()> {
    let (cx, cy) = (
        (square.left + square.right) / 2.0,
        (square.top + square.bottom) / 2.0,
    );
    let (hw, hh) = (12.0 * scale, 6.75 * scale);
    let rect = D2D_RECT_F {
        left: cx - hw,
        top: cy - hh,
        right: cx + hw,
        bottom: cy + hh,
    };
    // shortcut: uploaded on each paint of the Shelf page (a few per second at most); cache it like the
    // chip's thumbnails if the page ever animates for long.
    let bitmap = upload(context, pixels)?;
    thumb(context, rect, hh, &bitmap, pixels.width, opacity)
}

/// `rect` grown by `factor` about its centre.
fn grown(rect: D2D_RECT_F, factor: f32) -> D2D_RECT_F {
    let (cx, cy) = (
        (rect.left + rect.right) / 2.0,
        (rect.top + rect.bottom) / 2.0,
    );
    let (hw, hh) = (
        (rect.right - rect.left) / 2.0 * factor,
        (rect.bottom - rect.top) / 2.0 * factor,
    );
    D2D_RECT_F {
        left: cx - hw,
        top: cy - hh,
        right: cx + hw,
        bottom: cy + hh,
    }
}

/// Draws the chip while the pill shows; nothing otherwise.
pub fn chip(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    p: &Palette,
    thumbs: &Thumbs,
) -> Result<()> {
    let reveal = motion::reveal(scene.progress);
    let Some(shot) = scene.shot.as_ref().filter(|_| reveal > 0.0) else {
        return Ok(());
    };
    let alpha = reveal * scene.fx.notice;
    let (first, second) = shots::lines(shot.count, shot.dragging);
    let font = |size: f32| Font {
        family: TEXT_FONT,
        size: size * scale,
        weight: DWRITE_FONT_WEIGHT_MEDIUM,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_LEADING,
    };
    let (big, small) = (font(12.0), font(10.5));
    let text = text_width(&first, &big)?.max(text_width(second, &small)?) / scale;
    // 4 inset, 64 thumbnail, 10 gap, the text, 18 right pad.
    layout::set_shot_width((96.0 + text).ceil() as u16);
    let slot = layout::shot_chip(
        scene.track.is_some(),
        scene.dpi,
        (body.right - body.left) as i32,
    );
    let left = body.left + slot.x as f32;
    let top = body.top + slot.y as f32;
    let chip = D2D_RECT_F {
        left,
        top,
        right: left + slot.width as f32,
        bottom: top + slot.height as f32,
    };
    let height = chip.bottom - chip.top;
    let Rgba(r, g, b, a) = p.tile;
    fill(context, chip, height / 2.0, rgba(r, g, b, a * alpha))?;
    if shot.hover > 0.0 {
        let ink = with_alpha(p.ink, p.ink_alpha(0.07 * shot.hover * alpha));
        fill(context, chip, height / 2.0, ink)?;
    }
    let inset = 4.0 * scale;
    let slot_rect = D2D_RECT_F {
        left: chip.left + inset,
        top: chip.top + inset,
        right: chip.left + inset + 64.0 * scale,
        bottom: chip.top + inset + 36.0 * scale,
    };
    let radius = 18.0 * scale;
    let mut bitmaps = thumbs.borrow_mut();
    let [top_slot, under_slot] = &mut *bitmaps;
    if shot.dragging {
        fill(
            context,
            slot_rect,
            radius,
            with_alpha(p.ink, p.ink_alpha(0.12 * alpha)),
        )?;
    } else {
        let land = shot.land.clamp(0.0, 1.2);
        let factor = (0.92 + 0.08 * land) * (1.0 + 0.04 * shot.hover);
        let under = cached(under_slot, shot.under, |pixels| upload(context, pixels))?;
        if let (Some(bitmap), Some((_, pixels))) = (under, shot.under) {
            let peek = D2D_RECT_F {
                left: slot_rect.left + 8.0 * scale,
                top: slot_rect.top - 5.0 * scale,
                right: slot_rect.right + 8.0 * scale,
                bottom: slot_rect.bottom - 5.0 * scale,
            };
            thumb(context, peek, radius, &bitmap, pixels.width, 0.55 * alpha)?;
            // A 2 px `body` ring sets the top one apart.
            let Rgba(r, g, b, _) = p.body;
            let ring = grown(slot_rect, factor);
            let ring = D2D_RECT_F {
                left: ring.left - 2.0 * scale,
                top: ring.top - 2.0 * scale,
                right: ring.right + 2.0 * scale,
                bottom: ring.bottom + 2.0 * scale,
            };
            fill(context, ring, radius + 2.0 * scale, rgba(r, g, b, alpha))?;
        }
        if let Some(bitmap) = cached(top_slot, Some(shot.top), |pixels| upload(context, pixels))? {
            let rect = grown(slot_rect, factor);
            thumb(
                context,
                rect,
                radius * factor,
                &bitmap,
                shot.top.1.width,
                alpha,
            )?;
        }
    }
    let text_alpha = alpha * shot.land.clamp(0.0, 1.0);
    let x = slot_rect.right + 10.0 * scale;
    let line = |dy: f32, h: f32| D2D_RECT_F {
        left: x,
        top: chip.top + dy * scale,
        right: chip.right - 18.0 * scale,
        bottom: chip.top + (dy + h) * scale,
    };
    draw_text(
        context,
        &first,
        &big,
        line(6.0, 17.0),
        with_alpha(p.ink, p.ink_alpha(text_alpha)),
    )?;
    draw_text(
        context,
        second,
        &small,
        line(23.0, 15.0),
        with_alpha(p.ink, p.ink_alpha(0.6 * text_alpha)),
    )
}
