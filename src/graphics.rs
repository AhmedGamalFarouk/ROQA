use crate::{
    dock,
    icons::IconPixels,
    layout::{self, Bounds},
    motion,
};
use std::mem::ManuallyDrop;
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::{
    Foundation::{E_FAIL, HMODULE, HWND, POINT},
    Graphics::{
        Direct2D::{
            CLSID_D2D1Shadow,
            Common::{
                D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F,
                D2D1_COMPOSITE_MODE_SOURCE_OVER, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
                D2D1_PIXEL_FORMAT,
            },
            D2D1_ANTIALIAS_MODE_ALIASED, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT,
            D2D1_ARC_SIZE_SMALL, D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_PROPERTIES1,
            D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
            D2D1_INTERPOLATION_MODE_LINEAR, D2D1_PROPERTY_TYPE_FLOAT, D2D1_PROPERTY_TYPE_VECTOR4,
            D2D1_ROUNDED_RECT, D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION, D2D1_SHADOW_PROP_COLOR,
            D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
            D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateDevice, ID2D1DeviceContext,
            ID2D1PathGeometry,
        },
        Direct3D::D3D_DRIVER_TYPE_HARDWARE,
        Direct3D11::{
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
        },
        DirectComposition::{
            DCompositionCreateDevice2, IDCompositionDesktopDevice, IDCompositionSurface,
            IDCompositionTarget, IDCompositionVisual, IDCompositionVisual2,
        },
        DirectWrite::{
            DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_RANGE,
            DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
            DWriteCreateFactory, IDWriteFactory,
        },
        Dxgi::{
            Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM},
            IDXGIDevice,
        },
    },
};
use windows::core::{Error, Interface, PCWSTR, Result, w};
use windows_numerics::Vector2;

#[derive(Clone)]
pub struct Graphics(pub IDCompositionDesktopDevice);
impl Graphics {
    pub fn create() -> Result<Self> {
        let mut d3d = None;
        // SAFETY: output storage is valid; hardware BGRA device is required, not silently emulated.
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
        }
        let d3d: ID3D11Device = d3d.ok_or_else(|| Error::new(E_FAIL, "D3D returned no device."))?;
        let dxgi: IDXGIDevice = d3d.cast()?;
        // SAFETY: D2D and composition retain device references; all use stays on the UI thread.
        unsafe {
            let d2d = D2D1CreateDevice(&dxgi, None)?;
            Ok(Self(DCompositionCreateDevice2(&d2d)?))
        }
    }
}

/// What one dock slot shows. Slot 0 is the picker glyph; the others are real app groups.
pub struct DockSlot<'a> {
    pub icon: Option<&'a IconPixels>,
    pub active: bool,
    pub picker: bool,
    /// Hover animation progress, 0 (rest) to 1 (hovered).
    pub hover: f32,
}

pub struct DockScene<'a> {
    pub slots: Vec<DockSlot<'a>>,
    /// Pointer-inside animation progress for the bar fill; keeps the M1 hover background semantic.
    pub inside: f32,
    pub dpi: u32,
}

pub struct NotchScene<'a> {
    pub time: &'a str,
    pub date: &'a str,
    /// 0 collapsed, 1 expanded; the open curve briefly overshoots 1.
    pub progress: f32,
    pub dpi: u32,
}

/// Dock material (docs/design/roqa-visual-spec.md): black bar, white-alpha hover fill, whole-pixel icon lift.
const DOCK_RADIUS_DIPS: i32 = 20;
const DOCK_HOVER_FILL: f32 = 20.0;
const ICON_LIFT_DIPS: f32 = 3.0;
const WHITE: (u8, u8, u8) = (255, 255, 255);
const TEXT_FONT: PCWSTR = w!("Segoe UI Variable Text");
const DISPLAY_FONT: PCWSTR = w!("Segoe UI Variable Display");
/// The time's digits lead; "pm" and similar designators sit smaller beside them (as measured on Nusa).
const TIME_DESIGNATOR: f32 = 0.72;

pub struct Canvas {
    target: IDCompositionTarget,
    visual: IDCompositionVisual2,
    surface: IDCompositionSurface,
    graphics: Graphics,
    width: u32,
    height: u32,
}
impl Canvas {
    pub fn create(graphics: &Graphics, hwnd: HWND, bounds: Bounds) -> Result<Self> {
        // SAFETY: owned HWND outlives canvas; dimensions are positive validated layout.
        unsafe {
            let target = graphics.0.CreateTargetForHwnd(hwnd, true)?;
            let visual = graphics.0.CreateVisual()?;
            let surface = create_surface(graphics, bounds)?;
            visual.SetContent(&surface)?;
            target.SetRoot(&visual)?;
            let canvas = Self {
                target,
                visual,
                surface,
                graphics: graphics.clone(),
                width: bounds.width as u32,
                height: bounds.height as u32,
            };
            // Transparent until the host paints real content, so no placeholder rectangle flashes.
            canvas.draw(|_, _| Ok(()))?;
            Ok(canvas)
        }
    }
    /// Replaces the surface at a new size; the HWND, target and visual are kept.
    pub fn resize(&mut self, bounds: Bounds) -> Result<()> {
        let surface = create_surface(&self.graphics, bounds)?;
        // SAFETY: owned visual on the UI thread; the old surface is released after the swap.
        unsafe {
            self.visual.SetContent(&surface)?;
        }
        self.surface = surface;
        self.width = bounds.width as u32;
        self.height = bounds.height as u32;
        Ok(())
    }
    /// Shadow is composed on a separate, input-transparent window.
    pub fn paint_notch_shadow(&self, scene: &NotchScene) -> Result<()> {
        let scale = scene.dpi as f32 / 96.0;
        let shape = layout::notch_shape(scene.progress);
        let pixels = layout::notch_body(self.width as i32, scene.dpi, shape);
        self.draw(|context, rect| {
            let body = D2D_RECT_F {
                left: rect.left + pixels.left as f32,
                top: rect.top,
                right: rect.left + (pixels.left + pixels.width) as f32,
                bottom: rect.top + pixels.height as f32,
            };
            let open = motion::smooth(scene.progress);
            if open > 0.0 {
                notch_shadow(context, &body, shape.radius * scale, scale, open)?;
            }
            Ok(())
        })
    }
    /// Black notch with concave top ears and a persistent morphing clock.
    pub fn paint_notch(&self, scene: &NotchScene) -> Result<()> {
        let scale = scene.dpi as f32 / 96.0;
        let shape = layout::notch_shape(scene.progress);
        // The same whole-pixel body that layout::notch_region and notch_hit use.
        let pixels = layout::notch_body(self.width as i32, scene.dpi, shape);
        self.draw(|context, rect| {
            let left = rect.left + pixels.left as f32;
            let body = D2D_RECT_F {
                left,
                top: rect.top,
                right: left + pixels.width as f32,
                bottom: rect.top + pixels.height as f32,
            };
            let outline = notch_outline(
                context,
                &body,
                layout::notch_radius(pixels, shape, scene.dpi),
                pixels.ears as f32,
            )?;
            // SAFETY: context is live between surface BeginDraw/EndDraw; the clip is popped below.
            unsafe {
                let black = context.CreateSolidColorBrush(&rgba(0, 0, 0, 1.0), None)?;
                context.FillGeometry(&outline, &black, None);
                context.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                context.PushAxisAlignedClip(&body, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            }
            let drawn = notch_text(context, scene, &body, scale);
            // SAFETY: balances the PushAxisAlignedClip above on the same live context.
            unsafe { context.PopAxisAlignedClip() };
            drawn
        })
    }
    pub fn paint_dock(&self, scene: &DockScene) -> Result<()> {
        self.draw(|context, rect| {
            let px = |dips: i32| dock::px(dips, scene.dpi) as f32;
            let fill = (DOCK_HOVER_FILL * scene.inside).round() as u8;
            let radius = px(DOCK_RADIUS_DIPS);
            // A one-physical-pixel inner hairline separates the black bar from dark wallpapers.
            let edge = D2D_RECT_F {
                left: rect.left + 0.5,
                top: rect.top + 0.5,
                right: rect.right - 0.5,
                bottom: rect.bottom - 0.5,
            };
            // SAFETY: context is live between surface BeginDraw/EndDraw.
            unsafe {
                let bar = context
                    .CreateSolidColorBrush(&rgba(fill, fill, fill + fill / 10, 1.0), None)?;
                context.FillRoundedRectangle(&rounded(rect, radius), &bar);
                let hairline = context.CreateSolidColorBrush(&with_alpha(WHITE, 0.10), None)?;
                context.DrawRoundedRectangle(&rounded(edge, radius - 0.5), &hairline, 1.0, None);
            }
            let visible = dock::visible_slots(self.width as i32, scene.dpi, scene.slots.len());
            for (index, slot) in scene.slots.iter().take(visible).enumerate() {
                let left = rect.left + px(dock::PAD_DIPS) + px(dock::SLOT_DIPS) * index as f32;
                let cell = D2D_RECT_F {
                    left,
                    top: rect.top,
                    right: left + px(dock::SLOT_DIPS),
                    bottom: rect.bottom,
                };
                draw_slot(context, &cell, slot, scene.dpi, &px)?;
            }
            Ok(())
        })
    }
    fn draw(
        &self,
        content: impl FnOnce(&ID2D1DeviceContext, D2D_RECT_F) -> Result<()>,
    ) -> Result<()> {
        let mut offset = POINT::default();
        // SAFETY: surface has no outstanding draw; context is scoped to this draw and EndDraw always runs.
        unsafe {
            let context: ID2D1DeviceContext = self.surface.BeginDraw(None, &mut offset)?;
            context.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            let rect = D2D_RECT_F {
                left: offset.x as f32,
                top: offset.y as f32,
                right: offset.x as f32 + self.width as f32,
                bottom: offset.y as f32 + self.height as f32,
            };
            // Shapes have transparent surroundings; clear only this surface's update rectangle.
            context.PushAxisAlignedClip(&rect, D2D1_ANTIALIAS_MODE_ALIASED);
            context.Clear(Some(&rgba(0, 0, 0, 0.0)));
            context.PopAxisAlignedClip();
            let drawing = content(&context, rect);
            let ended = self.surface.EndDraw();
            drawing?;
            ended?;
            self.graphics.0.Commit()
        }
    }
}
impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: detaches the tree while HWND and composition device are still alive.
        unsafe {
            if let Err(error) = self.target.SetRoot(None::<&IDCompositionVisual>) {
                eprintln!("Detach: {error}");
            }
            if let Err(error) = self.graphics.0.Commit() {
                eprintln!("Composition shutdown: {error}");
            }
        }
    }
}

fn create_surface(graphics: &Graphics, bounds: Bounds) -> Result<IDCompositionSurface> {
    // SAFETY: dimensions are positive validated layout; premultiplied BGRA matches all content.
    unsafe {
        graphics.0.CreateSurface(
            bounds.width as u32,
            bounds.height as u32,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_ALPHA_MODE_PREMULTIPLIED,
        )
    }
}

/// Body with rounded bottom corners, plus concave quarter-circle ears flaring into the top edge.
fn notch_outline(
    context: &ID2D1DeviceContext,
    body: &D2D_RECT_F,
    radius: f32,
    ears: f32,
) -> Result<ID2D1PathGeometry> {
    let (left, top, right, bottom) = (body.left, body.top, body.right, body.bottom);
    let radius = radius
        .min(bottom - top - ears)
        .min((right - left) / 2.0)
        .max(0.0);
    let point = |x: f32, y: f32| Vector2 { X: x, Y: y };
    let arc = |x: f32, y: f32, size: f32, clockwise: bool| D2D1_ARC_SEGMENT {
        point: point(x, y),
        size: D2D_SIZE_F {
            width: size,
            height: size,
        },
        rotationAngle: 0.0,
        sweepDirection: if clockwise {
            D2D1_SWEEP_DIRECTION_CLOCKWISE
        } else {
            D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
        },
        arcSize: D2D1_ARC_SIZE_SMALL,
    };
    // SAFETY: the factory, geometry and sink are local COM objects; the sink is closed before use.
    unsafe {
        let path = context.GetFactory()?.CreatePathGeometry()?;
        let sink = path.Open()?;
        sink.BeginFigure(point(left - ears, top), D2D1_FIGURE_BEGIN_FILLED);
        sink.AddArc(&arc(left, top + ears, ears, true));
        sink.AddLine(point(left, bottom - radius));
        sink.AddArc(&arc(left + radius, bottom, radius, false));
        sink.AddLine(point(right - radius, bottom));
        sink.AddArc(&arc(right, bottom - radius, radius, false));
        sink.AddLine(point(right, top + ears));
        sink.AddArc(&arc(right + ears, top, ears, true));
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        Ok(path)
    }
}

/// Soft shadow under the opening notch, fading in with it (the flush pill casts none).
/// Source: the body inset 6 DIP at the sides and ending 7 DIP above its bottom, blurred (sigma 20 DIP);
/// fitted to the live Nusa shadow profile (below and beside the body) in docs/design/nusa-reference.md.
fn notch_shadow(
    context: &ID2D1DeviceContext,
    body: &D2D_RECT_F,
    radius: f32,
    scale: f32,
    strength: f32,
) -> Result<()> {
    let source = D2D_RECT_F {
        left: body.left + 6.0 * scale,
        top: body.top,
        right: body.right - 6.0 * scale,
        bottom: body.bottom - 7.0 * scale,
    };
    if source.right <= source.left {
        return Ok(());
    }
    let sigma = 20.0 * scale;
    let color: Vec<u8> = [0.0f32, 0.0, 0.0, 0.9 * strength]
        .iter()
        .flat_map(|channel| channel.to_le_bytes())
        .collect();
    // SAFETY: context is live between surface BeginDraw/EndDraw; the original target is restored before
    // drawing, and the command list and effect are local COM objects released after this draw.
    unsafe {
        let shape = context.CreateCommandList()?;
        let target = context.GetTarget()?;
        context.SetTarget(&shape);
        let black = context.CreateSolidColorBrush(&rgba(0, 0, 0, 1.0), None)?;
        context.FillRoundedRectangle(&rounded(source, radius), &black);
        context.SetTarget(&target);
        shape.Close()?;
        let shadow = context.CreateEffect(&CLSID_D2D1Shadow)?;
        shadow.SetInput(0, &shape, true);
        shadow.SetValue(
            D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32,
            D2D1_PROPERTY_TYPE_FLOAT,
            &sigma.to_le_bytes(),
        )?;
        shadow.SetValue(
            D2D1_SHADOW_PROP_COLOR.0 as u32,
            D2D1_PROPERTY_TYPE_VECTOR4,
            &color,
        )?;
        context.DrawImage(
            &shadow.GetOutput()?,
            None,
            None,
            D2D1_INTERPOLATION_MODE_LINEAR,
            D2D1_COMPOSITE_MODE_SOURCE_OVER,
        );
    }
    Ok(())
}

/// One bold clock morphs from the pill into the panel (no crossfade gap, like Nusa's persistent clock):
/// it grows from 15 to 30 DIP and moves from the pill's middle to the panel's time row. Only the date
/// fades in, settling 6 DIP downward once the body has opened.
fn notch_text(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
) -> Result<()> {
    let open = motion::smooth(scene.progress);
    let row = |middle: f32, half: f32| D2D_RECT_F {
        left: body.left + 12.0 * scale,
        top: body.top + (middle - half) * scale,
        right: body.right - 12.0 * scale,
        bottom: body.top + (middle + half) * scale,
    };
    let size = 15.0 + 15.0 * open;
    let clock = Font {
        // Text optical size for the small pill clock, Display once it is large.
        family: if size < 20.0 { TEXT_FONT } else { DISPLAY_FONT },
        size: size * scale,
        weight: DWRITE_FONT_WEIGHT_BOLD,
        designator: TIME_DESIGNATOR,
    };
    draw_text(
        context,
        scene.time,
        &clock,
        row(15.0 + 14.0 * open, 24.0),
        with_alpha(WHITE, 1.0),
    )?;
    let date = motion::smooth((scene.progress - 0.35) / 0.45);
    if date <= 0.0 {
        return Ok(());
    }
    let font = Font {
        family: TEXT_FONT,
        size: 13.0 * scale,
        weight: DWRITE_FONT_WEIGHT_NORMAL,
        designator: 1.0,
    };
    draw_text(
        context,
        scene.date,
        &font,
        row(58.0 - 6.0 * (1.0 - date), 10.0),
        with_alpha(WHITE, 0.60 * date),
    )
}

struct Font {
    family: PCWSTR,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
    /// Size factor for an AM/PM designator around a time's digits; 1.0 leaves text unchanged.
    designator: f32,
}

fn draw_text(
    context: &ID2D1DeviceContext,
    text: &str,
    font: &Font,
    rect: D2D_RECT_F,
    color: D2D1_COLOR_F,
) -> Result<()> {
    let mut locale = [0u16; 85];
    let text: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: shared system factory; locale buffer is NUL-terminated by the API (zeroed on failure).
    // Formats are created per paint: minute changes, hover changes and the frames of short animations.
    unsafe {
        GetUserDefaultLocaleName(&mut locale);
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let format = factory.CreateTextFormat(
            font.family,
            None,
            font.weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            font.size,
            PCWSTR(locale.as_ptr()),
        )?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        // Long localized dates end in an ellipsis instead of being cut mid-glyph by the body clip.
        let ellipsis = factory.CreateEllipsisTrimmingSign(&format)?;
        let trimming = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            ..Default::default()
        };
        format.SetTrimming(&trimming, &ellipsis)?;
        let layout = factory.CreateTextLayout(
            &text,
            &format,
            rect.right - rect.left,
            rect.bottom - rect.top,
        )?;
        if font.designator < 1.0 {
            for range in designator_ranges(&text) {
                layout.SetFontSize(font.size * font.designator, range)?;
            }
        }
        let brush = context.CreateSolidColorBrush(&color, None)?;
        context.DrawTextLayout(
            Vector2 {
                X: rect.left,
                Y: rect.top,
            },
            &layout,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );
    }
    Ok(())
}

/// UTF-16 ranges before the first and after the last digit of a formatted time, such as " pm" in
/// "9:50 pm" or "오후 " in "오후 10:00". Any script's digits count, so the user-locale format is kept.
fn designator_ranges(text: &[u16]) -> Vec<DWRITE_TEXT_RANGE> {
    let mut digits = None::<(usize, usize)>;
    let mut offset = 0;
    for character in char::decode_utf16(text.iter().copied()).flatten() {
        let end = offset + character.len_utf16();
        if character.is_numeric() {
            digits = Some((digits.map_or(offset, |(first, _)| first), end));
        }
        offset = end;
    }
    let Some((first, last)) = digits else {
        return Vec::new();
    };
    [(0, first), (last, text.len())]
        .into_iter()
        .filter(|(start, end)| end > start)
        .map(|(start, end)| DWRITE_TEXT_RANGE {
            startPosition: start as u32,
            length: (end - start) as u32,
        })
        .collect()
}

fn rounded(rect: D2D_RECT_F, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect,
        radiusX: radius,
        radiusY: radius,
    }
}

fn draw_slot(
    context: &ID2D1DeviceContext,
    cell: &D2D_RECT_F,
    slot: &DockSlot,
    dpi: u32,
    px: &dyn Fn(i32) -> f32,
) -> Result<()> {
    let size = px(dock::ICON_DIPS);
    // Whole-pixel lift keeps shell icon bitmaps unresampled (crisp) on every frame.
    let lift = (ICON_LIFT_DIPS * dpi as f32 / 96.0 * slot.hover).round();
    let center_x = (cell.left + cell.right) / 2.0;
    let center_y = (cell.top + cell.bottom) / 2.0 - lift;
    let frame = D2D_RECT_F {
        left: center_x - size / 2.0,
        top: center_y - size / 2.0,
        right: center_x + size / 2.0,
        bottom: center_y + size / 2.0,
    };
    // SAFETY: context is live between surface BeginDraw/EndDraw; bitmaps and brushes are scoped to this draw.
    unsafe {
        if slot.hover > 0.0 {
            // A 48-DIP chip centred on the (lifted) icon, so the two rise together.
            let half = px(24);
            let highlight = D2D_RECT_F {
                left: center_x - half,
                top: center_y - half,
                right: center_x + half,
                bottom: center_y + half,
            };
            let brush =
                context.CreateSolidColorBrush(&with_alpha(WHITE, 0.12 * slot.hover), None)?;
            context.FillRoundedRectangle(&rounded(highlight, px(12)), &brush);
        }
        let glyph = context.CreateSolidColorBrush(&rgba(220, 225, 232, 1.0), None)?;
        if slot.picker {
            // Our own glyph: a 2x2 grid meaning "all windows".
            let (tile, gap) = (px(12), px(4));
            for (column, row) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let left = center_x - tile - gap / 2.0 + column * (tile + gap);
                let top = center_y - tile - gap / 2.0 + row * (tile + gap);
                let square = D2D_RECT_F {
                    left,
                    top,
                    right: left + tile,
                    bottom: top + tile,
                };
                context.FillRoundedRectangle(&rounded(square, px(3)), &glyph);
            }
        } else if let Some(pixels) = slot.icon {
            // shortcut: bitmaps are rebuilt per paint (catalog/icon/hover changes and the frames of a ~0.2 s hover animation); cache per device if profiling shows cost.
            let properties = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
                colorContext: ManuallyDrop::new(None),
            };
            let bitmap = context.CreateBitmap(
                D2D_SIZE_U {
                    width: pixels.width,
                    height: pixels.height,
                },
                Some(pixels.pixels.as_ptr().cast()),
                pixels.width * 4,
                &properties,
            )?;
            context.DrawBitmap(
                &bitmap,
                Some(&frame),
                1.0,
                D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                None,
                None,
            );
        } else {
            // A real group whose icon is unavailable or still loading: neutral tile, never a fake app icon.
            let brush = context.CreateSolidColorBrush(&rgba(58, 58, 64, 1.0), None)?;
            context.FillRoundedRectangle(&rounded(frame, px(10)), &brush);
        }
        if slot.active {
            // Short white pill under the focused group.
            let (half, bottom) = (px(6), cell.bottom - px(4));
            let pill = D2D_RECT_F {
                left: center_x - half,
                top: bottom - px(3),
                right: center_x + half,
                bottom,
            };
            let brush = context.CreateSolidColorBrush(&with_alpha(WHITE, 0.92), None)?;
            context.FillRoundedRectangle(&rounded(pill, px(3) / 2.0), &brush);
        }
    }
    Ok(())
}

fn with_alpha((red, green, blue): (u8, u8, u8), alpha: f32) -> D2D1_COLOR_F {
    rgba(red, green, blue, alpha)
}

fn rgba(red: u8, green: u8, blue: u8, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(red) / 255.0,
        g: f32::from(green) / 255.0,
        b: f32::from(blue) / 255.0,
        a: alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::designator_ranges;

    fn ranges(text: &str) -> Vec<(u32, u32)> {
        let units: Vec<u16> = text.encode_utf16().collect();
        designator_ranges(&units)
            .iter()
            .map(|range| (range.startPosition, range.length))
            .collect()
    }

    #[test]
    fn time_designators_shrink_around_digits_in_any_script() {
        assert_eq!(ranges("9:50 pm"), vec![(4, 3)]);
        assert_eq!(ranges("21:50"), vec![]);
        assert_eq!(ranges("\u{c624}\u{d6c4} 10:00"), vec![(0, 3)]);
        // Arabic-Indic digits with a trailing Arabic designator.
        assert_eq!(
            ranges("\u{661}\u{660}:\u{665}\u{660} \u{645}"),
            vec![(5, 2)]
        );
        assert_eq!(ranges(""), vec![]);
    }
}
