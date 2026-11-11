use crate::{
    audio::{BARS, Level},
    dock,
    icons::IconPixels,
    layout::{self, Bounds},
    media::{PLAY_PAUSE, Progress, Track},
    motion,
};
use std::{borrow::Cow, cell::RefCell, mem::ManuallyDrop};
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::{
    Foundation::{D2DERR_RECREATE_TARGET, E_FAIL, HMODULE, HWND, POINT},
    Graphics::{
        Direct2D::{
            CLSID_D2D1Shadow,
            Common::{
                D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F,
                D2D1_COMPOSITE_MODE_SOURCE_OVER, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
                D2D1_PIXEL_FORMAT,
            },
            D2D1_ANTIALIAS_MODE_ALIASED, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT,
            D2D1_ARC_SIZE_SMALL, D2D1_BITMAP_BRUSH_PROPERTIES1, D2D1_BITMAP_OPTIONS_NONE,
            D2D1_BITMAP_PROPERTIES1, D2D1_BRUSH_PROPERTIES, D2D1_DRAW_TEXT_OPTIONS_NONE,
            D2D1_EXTEND_MODE_CLAMP, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
            D2D1_INTERPOLATION_MODE_LINEAR, D2D1_PROPERTY_TYPE_FLOAT, D2D1_PROPERTY_TYPE_VECTOR4,
            D2D1_ROUNDED_RECT, D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION, D2D1_SHADOW_PROP_COLOR,
            D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
            D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateDevice, ID2D1Bitmap1, ID2D1DeviceContext,
            ID2D1PathGeometry,
        },
        Direct3D::D3D_DRIVER_TYPE_HARDWARE,
        Direct3D11::{
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
        },
        DirectComposition::{
            DCompositionCreateDevice2, IDCompositionDesktopDevice, IDCompositionDevice,
            IDCompositionSurface, IDCompositionTarget, IDCompositionVisual, IDCompositionVisual2,
        },
        DirectWrite::{
            DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            DWRITE_READING_DIRECTION_RIGHT_TO_LEFT, DWRITE_TEXT_ALIGNMENT,
            DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE,
            DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
            DWriteCreateFactory, IDWriteFactory, IDWriteTextLayout,
        },
        Dxgi::{
            Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM},
            DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
            DXGI_ERROR_DRIVER_INTERNAL_ERROR, IDXGIDevice,
        },
    },
};
use windows::core::{Error, HRESULT, Interface, PCWSTR, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

#[derive(Clone)]
pub struct Graphics {
    pub composition: IDCompositionDesktopDevice,
    /// The device composition renders with, kept to ask whether it was removed.
    d3d: ID3D11Device,
}
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
            Ok(Self {
                composition: DCompositionCreateDevice2(&d2d)?,
                d3d,
            })
        }
    }
    /// Whether `error`, or the device itself, reports a device loss. A lost DXGI device also loses its
    /// composition device, so both devices and everything drawn with them must be created again.
    pub fn lost(&self, error: &Error) -> bool {
        is_loss(error.code()) || self.removed()
    }
    /// Queries device state without drawing; DirectComposition sends WM_PAINT to its windows on a loss.
    pub fn removed(&self) -> bool {
        // SAFETY: state queries on devices owned by this UI thread. The desktop device answers
        // IDCompositionDevice (DCompositionCreateDevice2); a failed query is not taken as a loss.
        unsafe {
            self.d3d.GetDeviceRemovedReason().is_err()
                || self
                    .composition
                    .cast::<IDCompositionDevice>()
                    .and_then(|device| device.CheckDeviceState())
                    .is_ok_and(|valid| !valid.as_bool())
        }
    }
}

/// Results meaning the device is gone (the GetDeviceRemovedReason codes, and Direct2D's own).
fn is_loss(code: HRESULT) -> bool {
    [
        DXGI_ERROR_DEVICE_REMOVED,
        DXGI_ERROR_DEVICE_RESET,
        DXGI_ERROR_DEVICE_HUNG,
        DXGI_ERROR_DRIVER_INTERNAL_ERROR,
        D2DERR_RECREATE_TARGET,
    ]
    .contains(&code)
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
    /// The media session's track, drawn in the wider media notch; `None` draws the plain clock notch.
    pub track: Option<&'a Track>,
    /// The track's album art, drawn in its cover slot, and its entry number (`Media::art`).
    pub art: Option<(u64, &'a IconPixels)>,
    /// 0 collapsed, 1 expanded; the open curve briefly overshoots 1.
    pub progress: f32,
    pub dpi: u32,
    /// The media control with keyboard focus, drawn with a focus ring.
    pub focus: Option<usize>,
    /// The panel's progress row; none while collapsed or without a track duration.
    pub played: Option<Progress>,
    /// The panel's volume row; `None` without an audio endpoint draws it disabled.
    pub volume: Option<Level>,
    /// The visualizer's bar heights in whole pixels (`layout::meter_heights`).
    pub bars: [i32; BARS],
}

/// Dock material (docs/design/roqa-visual-spec.md): black bar, white-alpha hover fill, whole-pixel icon lift.
const DOCK_RADIUS_DIPS: i32 = 20;
const DOCK_HOVER_FILL: f32 = 20.0;
const ICON_LIFT_DIPS: f32 = 3.0;
const WHITE: (u8, u8, u8) = (255, 255, 255);
const TEXT_FONT: PCWSTR = w!("Segoe UI Variable Text");
const DISPLAY_FONT: PCWSTR = w!("Segoe UI Variable Display");
/// The time's digits lead; "pm" and similar designators sit smaller beside them.
const TIME_DESIGNATOR: f32 = 0.72;

pub struct Canvas {
    target: IDCompositionTarget,
    visual: IDCompositionVisual2,
    surface: IDCompositionSurface,
    graphics: Graphics,
    width: u32,
    height: u32,
    /// The album art's device bitmap and its entry number. Reused while the entry is unchanged and
    /// released with the canvas, which a device loss or rebuild replaces.
    art: RefCell<Option<(u64, ID2D1Bitmap1)>>,
}
impl Canvas {
    pub fn create(graphics: &Graphics, hwnd: HWND, bounds: Bounds) -> Result<Self> {
        // SAFETY: owned HWND outlives canvas; dimensions are positive validated layout.
        unsafe {
            let target = graphics.composition.CreateTargetForHwnd(hwnd, true)?;
            let visual = graphics.composition.CreateVisual()?;
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
                art: RefCell::default(),
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
        let shape = layout::notch_shape(scene.progress, scene.track.is_some());
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
    /// Black notch with concave top ears, a persistent morphing clock and the media track, if any.
    pub fn paint_notch(&self, scene: &NotchScene) -> Result<()> {
        let scale = scene.dpi as f32 / 96.0;
        let shape = layout::notch_shape(scene.progress, scene.track.is_some());
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
            let art = cached(&mut self.art.borrow_mut(), scene.art, |pixels| {
                upload(context, pixels)
            });
            let drawn = art.and_then(|art| notch_text(context, scene, &body, scale, art.as_ref()));
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
            self.graphics.composition.Commit()
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
            if let Err(error) = self.graphics.composition.Commit() {
                eprintln!("Composition shutdown: {error}");
            }
        }
    }
}

fn create_surface(graphics: &Graphics, bounds: Bounds) -> Result<IDCompositionSurface> {
    // SAFETY: dimensions are positive validated layout; premultiplied BGRA matches all content.
    unsafe {
        graphics.composition.CreateSurface(
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

/// One bold clock morphs from the pill into the panel, never crossfading, so the time stays readable:
/// it grows from 15 to 30 DIP and moves from the pill's middle to the panel's time row. Only the date
/// fades in, settling 6 DIP downward once the body has opened. A media track draws `media_plan`
/// instead.
fn notch_text(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    art: Option<&ID2D1Bitmap1>,
) -> Result<()> {
    if let Some(track) = scene.track {
        let font = media_clock(motion::smooth(scene.progress), scale);
        let clock = text_width(scene.time, &font)?;
        return media_text(context, &media_plan(scene, track, body, clock), art);
    }
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
        align: DWRITE_TEXT_ALIGNMENT_CENTER,
    };
    draw_text(
        context,
        scene.time,
        &clock,
        row(15.0 + 14.0 * open, 24.0),
        with_alpha(WHITE, 1.0),
    )?;
    let date = motion::reveal(scene.progress);
    if date <= 0.0 {
        return Ok(());
    }
    let font = Font {
        family: TEXT_FONT,
        size: 13.0 * scale,
        weight: DWRITE_FONT_WEIGHT_NORMAL,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_CENTER,
    };
    draw_text(
        context,
        scene.date,
        &font,
        row(58.0 - 6.0 * (1.0 - date), 10.0),
        with_alpha(WHITE, 0.60 * date),
    )
}

/// Fill of the cover slot when the track has no album art; art fills the same geometry.
const COVER_FILL: f32 = 0.20;
/// Side of the panel cover, the largest drawn album art; the host decodes art for it.
pub const COVER_DIPS: f32 = 72.0;

/// One text element of the media notch, in physical pixels.
struct Line<'a> {
    text: Cow<'a, str>,
    font: Font,
    rect: D2D_RECT_F,
    alpha: f32,
}

/// What the media notch draws for one frame, in physical pixels: the cover slot with its centred
/// play-state glyph (a triangle while playing, two bars while paused), the visible text lines, then
/// the panel's controls once they fade in.
struct MediaPlan<'a> {
    cover: D2D1_ROUNDED_RECT,
    /// Album art fills the cover in place of the slot and its glyph.
    art: Option<&'a IconPixels>,
    /// The pill's play-state badge on the art.
    badge: Option<Badge>,
    glyph: D2D_RECT_F,
    playing: bool,
    /// Glyph opacity, and a factor of the title's: 1 playing, 0.6 paused.
    dim: f32,
    lines: Vec<Line<'a>>,
    /// The progress bar's track and filled part, and their opacity.
    bar: Option<([D2D1_ROUNDED_RECT; 2], f32)>,
    chips: Vec<Chip>,
    /// Around the control with keyboard focus.
    ring: Option<Ring>,
    volume: Option<VolumeBar>,
    /// The visualizer's bars and their opacity.
    meter: Option<([D2D1_ROUNDED_RECT; BARS], f32)>,
}

/// A black circle carrying the play-state glyph at the art's bottom-right corner, faded by `alpha`.
struct Badge {
    circle: D2D1_ROUNDED_RECT,
    glyph: D2D_RECT_F,
    alpha: f32,
}
/// The volume level bar: a thin track, and its fill to the shown percent, dimmed while muted or
/// without an endpoint. Both fade in with the controls.
struct VolumeBar {
    track: D2D1_ROUNDED_RECT,
    track_alpha: f32,
    fill: D2D1_ROUNDED_RECT,
    fill_alpha: f32,
}
/// The visualizer's bars: secondary, decorative content, white 60 % like the artist.
const METER_FILL: f32 = 0.60;
/// The bar's drawn thickness in DIPs, centred in its hit box.
const VOLUME_THICKNESS: f32 = 4.0;
/// The bar's track: white 40 % is about 3.7 : 1 on black, the non-text contrast a target needs.
const VOLUME_TRACK: f32 = 0.40;
/// The fill while muted: white 60 % over the track is about 3.2 : 1 against it, and dimmer than the
/// audible white fill.
const MUTED_FILL: f32 = 0.60;

/// One media control: a white glyph, or for play/pause a black glyph on a white circle. A control
/// the player does not accept now is drawn at `DISABLED` opacity.
struct Chip {
    rect: D2D_RECT_F,
    glyph: D2D_RECT_F,
    shape: Glyph,
    filled: bool,
    alpha: f32,
}
struct Ring {
    shape: D2D1_ROUNDED_RECT,
    stroke: f32,
    alpha: f32,
}
/// Opacity of a control the player does not accept now.
const DISABLED: f32 = 0.30;
/// Focus ring: stroke width and its gap from the control's edge, in DIPs.
const FOCUS_STROKE: f32 = 2.0;
const FOCUS_GAP: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Glyph {
    Play,
    Pause,
    Previous,
    Next,
    /// The mute toggle: a speaker with sound waves, or without them while muted.
    Speaker,
    Muted,
}
/// Glyph parts on a 10-unit grid: a triangle's x, y corners, and a bar's left, top, right, bottom.
type Triangle = [(f32, f32); 3];
type Bar = (f32, f32, f32, f32);
impl Glyph {
    fn parts(self) -> (Option<Triangle>, &'static [Bar]) {
        match self {
            Glyph::Play => (Some([(2.5, 1.0), (9.0, 5.0), (2.5, 9.0)]), &[]),
            Glyph::Pause => (None, &[(2.0, 1.0, 4.2, 9.0), (5.8, 1.0, 8.0, 9.0)]),
            Glyph::Previous => (
                Some([(9.0, 1.0), (9.0, 9.0), (3.8, 5.0)]),
                &[(1.5, 1.0, 3.1, 9.0)],
            ),
            Glyph::Next => (
                Some([(1.0, 1.0), (6.2, 5.0), (1.0, 9.0)]),
                &[(7.0, 1.0, 8.6, 9.0)],
            ),
            Glyph::Speaker => (
                Some([(0.5, 5.0), (6.5, 1.0), (6.5, 9.0)]),
                &[
                    (1.0, 3.5, 3.5, 6.5),
                    (7.6, 3.5, 8.4, 6.5),
                    (9.0, 2.0, 9.8, 8.0),
                ],
            ),
            Glyph::Muted => (
                Some([(0.5, 5.0), (6.5, 1.0), (6.5, 9.0)]),
                &[(1.0, 3.5, 3.5, 6.5)],
            ),
        }
    }
}

/// The media clock at `open` (`smooth(progress)`): the pill's 15 DIP bold clock settles into the
/// panel's 12 DIP regular one, trailing-aligned so it is never trimmed.
fn media_clock(open: f32, scale: f32) -> Font {
    Font {
        family: TEXT_FONT,
        size: (15.0 - 3.0 * open) * scale,
        weight: if open < 0.5 {
            DWRITE_FONT_WEIGHT_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        },
        designator: TIME_DESIGNATOR,
        align: DWRITE_TEXT_ALIGNMENT_TRAILING,
    }
}

/// Direction B ("Cover accent"; docs/design/roqa-visual-spec.md, "Media notch"). Every element
/// moves by `smooth(p)` from its pill position to its panel position, in DIPs from the body's left
/// and top edges (the clock from its right edge). The title column ends before the clock's measured
/// width (`clock`, physical pixels) and never inverts. The artist fades in below the title as the
/// date does, and so do the controls (`media_chips`), the progress row (`progress_row`) and the
/// volume bar (`volume_bar`). Empty text is skipped.
fn media_plan<'a>(
    scene: &NotchScene<'a>,
    track: &'a Track,
    body: &D2D_RECT_F,
    clock: f32,
) -> MediaPlan<'a> {
    let scale = scene.dpi as f32 / 96.0;
    let open = motion::smooth(scene.progress);
    let at = |pill: f32, panel: f32| pill + (panel - pill) * open;
    // The close spring's undershoot briefly makes the pill shorter than 30: keep its row centred.
    let middle = (layout::notch_shape(scene.progress, true).height / 2.0).min(15.0);
    // Cover: 20 at (10, middle - 10), radius 5 -> COVER_DIPS at (16, 14), radius 10. Glyph: 10 -> 24.
    let size = (at(20.0, COVER_DIPS) * scale).round();
    let cover = pixel_square(
        body.left + at(10.0, 16.0) * scale,
        body.top + at(middle - 10.0, 14.0) * scale,
        size,
    );
    let glyph = (at(10.0, 24.0) * scale).round();
    let inset = ((size - glyph) / 2.0).round();
    // Badge: 11 with a 6 glyph, 4 past the cover's right and bottom edges. Only the pill has one;
    // it fades out as the panel opens, whose play/pause control shows the state.
    let badge = (scene.art.is_some() && open < 1.0).then(|| {
        let side = (11.0 * scale).round();
        let circle = pixel_square(
            cover.right + 4.0 * scale - side,
            cover.bottom + 4.0 * scale - side,
            side,
        );
        let glyph = (6.0 * scale).round();
        let inset = ((side - glyph) / 2.0).round();
        Badge {
            circle: rounded(circle, side / 2.0),
            glyph: pixel_square(circle.left + inset, circle.top + inset, glyph),
            alpha: 1.0 - open,
        }
    });
    // Clock: trailing edge 14 -> 16 from the right, centre middle -> 19. Title: from 38 -> 100,
    // ending 8 -> 6 before the clock, centre middle -> 21.
    let end = body.right - at(14.0, 16.0) * scale;
    let leading = body.left + at(38.0, 100.0) * scale;
    let title = (leading, (end - clock - at(8.0, 6.0) * scale).max(leading));
    let row = |(left, right): (f32, f32), middle: f32, half: f32| D2D_RECT_F {
        left,
        top: body.top + (middle - half) * scale,
        right,
        bottom: body.top + (middle + half) * scale,
    };
    let font = |size: f32, weight| Font {
        family: TEXT_FONT,
        size: size * scale,
        weight,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_LEADING,
    };
    let dim = if track.playing { 1.0 } else { 0.60 };
    let artist = motion::reveal(scene.progress);
    let mut lines = vec![
        Line {
            text: scene.time.into(),
            font: media_clock(open, scale),
            rect: row((cover.right, end), at(middle, 19.0), 12.0),
            alpha: at(1.0, 0.60),
        },
        Line {
            text: track.title.as_str().into(),
            font: font(
                at(12.0, 13.0),
                if open < 0.5 {
                    DWRITE_FONT_WEIGHT_NORMAL
                } else {
                    DWRITE_FONT_WEIGHT_SEMI_BOLD
                },
            ),
            rect: row(title, at(middle, 21.0), 12.0),
            alpha: at(0.85, 1.0) * dim,
        },
        Line {
            text: track.artist.as_str().into(),
            font: font(12.0, DWRITE_FONT_WEIGHT_NORMAL),
            rect: row(title, 38.0 + 6.0 * (1.0 - artist), 10.0),
            alpha: 0.60 * artist,
        },
    ];
    let bar = progress_row(&mut lines, scene, body);
    let chips = media_chips(scene, track, body);
    let ring = scene
        .focus
        .and_then(|index| chips.get(index))
        .map(|chip| focus_ring(chip, scale, motion::reveal(scene.progress)));
    MediaPlan {
        cover: rounded(cover, at(5.0, 10.0) * scale),
        art: scene.art.map(|(_, pixels)| pixels),
        badge,
        glyph: pixel_square(cover.left + inset, cover.top + inset, glyph),
        playing: track.playing,
        dim,
        lines: lines
            .into_iter()
            .filter(|line| line.alpha > 0.0 && !line.text.is_empty())
            .collect(),
        bar,
        chips,
        ring,
        volume: volume_bar(scene, body),
        meter: meter_bars(scene, body),
    }
}

/// The visualizer right of the controls row, placed and revealed like it: fully rounded bars spread
/// across the slot (`layout::meter`), bottom-aligned at the scene's heights.
fn meter_bars(scene: &NotchScene, body: &D2D_RECT_F) -> Option<([D2D1_ROUNDED_RECT; BARS], f32)> {
    let reveal = motion::reveal(scene.progress);
    if reveal <= 0.0 {
        return None;
    }
    let (slot, bar) = layout::meter(scene.dpi);
    let bottom = body.top + (slot.y + slot.height) as f32;
    let bars = std::array::from_fn(|index| {
        let left =
            body.left + (slot.x + index as i32 * (slot.width - bar) / (BARS as i32 - 1)) as f32;
        let rect = D2D_RECT_F {
            left,
            top: bottom - scene.bars[index] as f32,
            right: left + bar as f32,
            bottom,
        };
        rounded(rect, bar as f32 / 2.0)
    });
    Some((bars, METER_FILL * reveal))
}

/// The panel's progress row (`layout::media_progress`), fading in with the controls: the elapsed
/// time in 11 DIP from x 100, the 4 DIP bar centred in its target, and the total time trailing at
/// 16 from the right, all centred on y 57. Adds the times to `lines` and returns the bar's track,
/// fill and opacity; none while the row is hidden or the scene has none.
fn progress_row(
    lines: &mut Vec<Line>,
    scene: &NotchScene,
    body: &D2D_RECT_F,
) -> Option<([D2D1_ROUNDED_RECT; 2], f32)> {
    let reveal = motion::reveal(scene.progress);
    let played = scene.played.filter(|_| reveal > 0.0)?;
    let scale = scene.dpi as f32 / 96.0;
    let target = layout::media_progress(scene.dpi);
    let thickness = (4.0 * scale).round();
    let left = body.left + target.x as f32;
    let top = (body.top + target.y as f32 + (target.height as f32 - thickness) / 2.0).round();
    let track = D2D_RECT_F {
        left,
        top,
        right: left + target.width as f32,
        bottom: top + thickness,
    };
    let fill = D2D_RECT_F {
        right: left + played.filled as f32,
        ..track
    };
    let [elapsed, total] = played.times();
    for (text, from, to, align) in [
        (elapsed, 100.0, 136.0, DWRITE_TEXT_ALIGNMENT_LEADING),
        (total, 308.0, 344.0, DWRITE_TEXT_ALIGNMENT_TRAILING),
    ] {
        lines.push(Line {
            text: text.into(),
            font: Font {
                family: TEXT_FONT,
                size: 11.0 * scale,
                weight: DWRITE_FONT_WEIGHT_NORMAL,
                designator: 1.0,
                align,
            },
            rect: D2D_RECT_F {
                left: body.left + from * scale,
                top: body.top + 49.0 * scale,
                right: body.left + to * scale,
                bottom: body.top + 65.0 * scale,
            },
            alpha: 0.60 * reveal,
        });
    }
    let radius = thickness / 2.0;
    Some(([rounded(track, radius), rounded(fill, radius)], reveal))
}

/// The level bar right of the mute toggle, placed and revealed like the controls; the fill ends at
/// the shown whole percent.
fn volume_bar(scene: &NotchScene, body: &D2D_RECT_F) -> Option<VolumeBar> {
    let reveal = motion::reveal(scene.progress);
    if reveal <= 0.0 {
        return None;
    }
    let bar = layout::volume_bar(scene.dpi);
    let thickness = (VOLUME_THICKNESS * scene.dpi as f32 / 96.0).round();
    let left = body.left + bar.x as f32;
    let top = body.top + bar.y as f32 + ((bar.height as f32 - thickness) / 2.0).round();
    let percent = scene.volume.map_or(0, |level| level.percent);
    let filled = (bar.width as f32 * f32::from(percent) / 100.0).round();
    let rect = |width: f32| {
        rounded(
            D2D_RECT_F {
                left,
                top,
                right: left + width,
                bottom: top + thickness,
            },
            thickness / 2.0,
        )
    };
    Some(VolumeBar {
        track: rect(bar.width as f32),
        track_alpha: reveal * VOLUME_TRACK,
        fill: rect(filled),
        fill_alpha: reveal
            * match scene.volume {
                Some(level) if !level.muted => 1.0,
                Some(_) => MUTED_FILL,
                None => DISABLED,
            },
    })
}

/// Previous, play/pause and next keep their panel place in the body, so the body clip reveals
/// them as it opens; they fade in with the artist. None are planned while they are invisible.
fn media_chips(scene: &NotchScene, track: &Track, body: &D2D_RECT_F) -> Vec<Chip> {
    let scale = scene.dpi as f32 / 96.0;
    let reveal = motion::reveal(scene.progress);
    if reveal <= 0.0 {
        return Vec::new();
    }
    let toggle = if track.playing {
        Glyph::Pause
    } else {
        Glyph::Play
    };
    let speaker = match scene.volume {
        Some(level) if level.muted => Glyph::Muted,
        _ => Glyph::Speaker,
    };
    let [previous, play_pause, next] = track.controls;
    let enabled = [previous, play_pause, next, scene.volume.is_some()];
    layout::media_controls(scene.dpi)
        .into_iter()
        .zip([Glyph::Previous, toggle, Glyph::Next, speaker])
        .enumerate()
        .map(|(index, (control, shape))| {
            let rect = D2D_RECT_F {
                left: body.left + control.x as f32,
                top: body.top + control.y as f32,
                right: body.left + (control.x + control.width) as f32,
                bottom: body.top + (control.y + control.height) as f32,
            };
            let filled = index == PLAY_PAUSE;
            // A 12 DIP glyph inside the circle, 14 DIP on its own.
            let size = (if filled { 12.0 } else { 14.0 } * scale).round();
            let centre = |extent: i32| ((extent as f32 - size) / 2.0).round();
            Chip {
                rect,
                glyph: pixel_square(
                    rect.left + centre(control.width),
                    rect.top + centre(control.height),
                    size,
                ),
                shape,
                filled,
                alpha: reveal * if enabled[index] { 1.0 } else { DISABLED },
            }
        })
        .collect()
}

/// The keyboard focus ring around `chip`: a circle around play/pause, a rounded rect otherwise.
fn focus_ring(chip: &Chip, scale: f32, alpha: f32) -> Ring {
    let stroke = FOCUS_STROKE * scale;
    // The stroke is centred on the path: its inner edge sits FOCUS_GAP outside the control.
    let out = FOCUS_GAP * scale + stroke / 2.0;
    let rect = D2D_RECT_F {
        left: chip.rect.left - out,
        top: chip.rect.top - out,
        right: chip.rect.right + out,
        bottom: chip.rect.bottom + out,
    };
    let radius = if chip.filled {
        (rect.bottom - rect.top) / 2.0
    } else {
        8.0 * scale + out
    };
    Ring {
        shape: rounded(rect, radius),
        stroke,
        alpha,
    }
}

fn media_text(
    context: &ID2D1DeviceContext,
    plan: &MediaPlan,
    bitmap: Option<&ID2D1Bitmap1>,
) -> Result<()> {
    let state = if plan.playing {
        Glyph::Play
    } else {
        Glyph::Pause
    };
    if let (Some(art), Some(bitmap)) = (plan.art, bitmap) {
        fill_art(context, &plan.cover, art.width, bitmap, plan.dim)?;
    } else {
        // SAFETY: context is live between surface BeginDraw/EndDraw; brushes are scoped to this draw.
        unsafe {
            let slot = context.CreateSolidColorBrush(&with_alpha(WHITE, COVER_FILL), None)?;
            context.FillRoundedRectangle(&plan.cover, &slot);
        }
        fill_glyph(context, plan.glyph, state, with_alpha(WHITE, plan.dim))?;
    }
    if let Some(badge) = &plan.badge {
        // SAFETY: as above.
        unsafe {
            let circle = context.CreateSolidColorBrush(&rgba(0, 0, 0, badge.alpha), None)?;
            context.FillRoundedRectangle(&badge.circle, &circle);
        }
        let color = with_alpha(WHITE, plan.dim * badge.alpha);
        fill_glyph(context, badge.glyph, state, color)?;
    }
    if let Some(([track, fill], alpha)) = &plan.bar {
        // SAFETY: as above.
        unsafe {
            let brush =
                context.CreateSolidColorBrush(&with_alpha(WHITE, COVER_FILL * alpha), None)?;
            context.FillRoundedRectangle(track, &brush);
            if fill.rect.right > fill.rect.left {
                let brush = context.CreateSolidColorBrush(&with_alpha(WHITE, *alpha), None)?;
                context.FillRoundedRectangle(fill, &brush);
            }
        }
    }
    for line in &plan.lines {
        draw_text(
            context,
            &line.text,
            &line.font,
            line.rect,
            with_alpha(WHITE, line.alpha),
        )?;
    }
    for chip in &plan.chips {
        let color = with_alpha(WHITE, chip.alpha);
        if chip.filled {
            let radius = (chip.rect.bottom - chip.rect.top) / 2.0;
            // SAFETY: as above.
            unsafe {
                let circle = context.CreateSolidColorBrush(&color, None)?;
                context.FillRoundedRectangle(&rounded(chip.rect, radius), &circle);
            }
            // Black on the circle; over the black body it vanishes as the circle fades.
            fill_glyph(context, chip.glyph, chip.shape, rgba(0, 0, 0, 1.0))?;
        } else {
            fill_glyph(context, chip.glyph, chip.shape, color)?;
        }
    }
    if let Some(bar) = &plan.volume {
        // SAFETY: as above.
        unsafe {
            let track = context.CreateSolidColorBrush(&with_alpha(WHITE, bar.track_alpha), None)?;
            context.FillRoundedRectangle(&bar.track, &track);
            let fill = context.CreateSolidColorBrush(&with_alpha(WHITE, bar.fill_alpha), None)?;
            context.FillRoundedRectangle(&bar.fill, &fill);
        }
    }
    if let Some((bars, alpha)) = &plan.meter {
        // SAFETY: as above.
        unsafe {
            let brush = context.CreateSolidColorBrush(&with_alpha(WHITE, *alpha), None)?;
            for bar in bars {
                context.FillRoundedRectangle(bar, &brush);
            }
        }
    }
    if let Some(ring) = &plan.ring {
        // SAFETY: as above.
        unsafe {
            let brush = context.CreateSolidColorBrush(&with_alpha(WHITE, ring.alpha), None)?;
            context.DrawRoundedRectangle(&ring.shape, &brush, ring.stroke, None);
        }
    }
    Ok(())
}

/// The bitmap for `art` from `slot`, uploading it only when its entry changed; no art releases the
/// held bitmap. A failed upload leaves the slot empty, so the next paint retries.
fn cached<T: Clone>(
    slot: &mut Option<(u64, T)>,
    art: Option<(u64, &IconPixels)>,
    upload: impl FnOnce(&IconPixels) -> Result<T>,
) -> Result<Option<T>> {
    let Some((key, pixels)) = art else {
        *slot = None;
        return Ok(None);
    };
    if let Some((held, bitmap)) = slot.as_ref()
        && *held == key
    {
        return Ok(Some(bitmap.clone()));
    }
    // The old bitmap is released before the new one is created.
    *slot = None;
    let bitmap = upload(pixels)?;
    *slot = Some((key, bitmap.clone()));
    Ok(Some(bitmap))
}

/// Album art, `side` pixels square, scaled into the cover square. A bitmap brush clips it to the
/// rounded rect directly, with no layer or geometry mask.
fn fill_art(
    context: &ID2D1DeviceContext,
    cover: &D2D1_ROUNDED_RECT,
    side: u32,
    bitmap: &ID2D1Bitmap1,
    opacity: f32,
) -> Result<()> {
    let scale = (cover.rect.right - cover.rect.left) / side as f32;
    let properties = D2D1_BITMAP_BRUSH_PROPERTIES1 {
        extendModeX: D2D1_EXTEND_MODE_CLAMP,
        extendModeY: D2D1_EXTEND_MODE_CLAMP,
        interpolationMode: D2D1_INTERPOLATION_MODE_LINEAR,
    };
    let placement = D2D1_BRUSH_PROPERTIES {
        opacity,
        transform: Matrix3x2 {
            M11: scale,
            M12: 0.0,
            M21: 0.0,
            M22: scale,
            M31: cover.rect.left,
            M32: cover.rect.top,
        },
    };
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw,
    // and the bitmap belongs to the same device.
    unsafe {
        let brush = context.CreateBitmapBrush(bitmap, Some(&properties), Some(&placement))?;
        // Cubic downscaling where the brush supports it; an unsupported mode is ignored, not an error.
        brush.SetInterpolationMode1(D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC);
        context.FillRoundedRectangle(cover, &brush);
    }
    Ok(())
}

/// A device bitmap copied from premultiplied BGRA pixels.
fn upload(context: &ID2D1DeviceContext, pixels: &IconPixels) -> Result<ID2D1Bitmap1> {
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
    // SAFETY: the buffer holds `height` rows of `width * 4` bytes and is copied before returning.
    unsafe {
        context.CreateBitmap(
            D2D_SIZE_U {
                width: pixels.width,
                height: pixels.height,
            },
            Some(pixels.pixels.as_ptr().cast()),
            pixels.width * 4,
            &properties,
        )
    }
}

/// A `size` square at whole physical pixels, as crisp as the whole-pixel body.
fn pixel_square(left: f32, top: f32, size: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: left.round(),
        top: top.round(),
        right: left.round() + size,
        bottom: top.round() + size,
    }
}

/// Fills `shape` on its 10-unit grid inside `square`; bars snap to whole pixels.
fn fill_glyph(
    context: &ID2D1DeviceContext,
    square: D2D_RECT_F,
    shape: Glyph,
    color: D2D1_COLOR_F,
) -> Result<()> {
    let unit = (square.right - square.left) / 10.0;
    let point = |(x, y): (f32, f32)| Vector2 {
        X: square.left + x * unit,
        Y: square.top + y * unit,
    };
    let (triangle, bars) = shape.parts();
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush and path geometry are
    // local COM objects scoped to this draw, and the sink is closed before the fill.
    unsafe {
        let brush = context.CreateSolidColorBrush(&color, None)?;
        if let Some([first, second, third]) = triangle {
            let path = context.GetFactory()?.CreatePathGeometry()?;
            let sink = path.Open()?;
            sink.BeginFigure(point(first), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLine(point(second));
            sink.AddLine(point(third));
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            context.FillGeometry(&path, &brush, None);
        }
        for &(left, top, right, bottom) in bars {
            let (top_left, bottom_right) = (point((left, top)), point((right, bottom)));
            let bar = D2D_RECT_F {
                left: top_left.X.round(),
                top: top_left.Y.round(),
                right: bottom_right.X.round(),
                bottom: bottom_right.Y.round(),
            };
            context.FillRectangle(&bar, &brush);
        }
    }
    Ok(())
}

struct Font {
    family: PCWSTR,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
    /// Size factor for an AM/PM designator around a time's digits; 1.0 leaves text unchanged.
    designator: f32,
    /// Leading text also takes its reading direction from its first letter; see `text_layout`.
    align: DWRITE_TEXT_ALIGNMENT,
}

fn draw_text(
    context: &ID2D1DeviceContext,
    text: &str,
    font: &Font,
    rect: D2D_RECT_F,
    color: D2D1_COLOR_F,
) -> Result<()> {
    let layout = text_layout(text, font, rect.right - rect.left, rect.bottom - rect.top)?;
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw.
    unsafe {
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

/// The untrimmed width of one line of `text`, in the font's units.
fn text_width(text: &str, font: &Font) -> Result<f32> {
    let mut metrics = DWRITE_TEXT_METRICS::default();
    // SAFETY: a local layout and a valid output struct.
    unsafe { text_layout(text, font, f32::MAX, font.size * 2.0)?.GetMetrics(&mut metrics)? };
    Ok(metrics.width)
}

/// One vertically centred line that ends in an ellipsis when it does not fit.
fn text_layout(text: &str, font: &Font, width: f32, height: f32) -> Result<IDWriteTextLayout> {
    let mut locale = [0u16; 85];
    let rtl = font.align == DWRITE_TEXT_ALIGNMENT_LEADING && right_to_left(text);
    let text: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: shared system factory; locale buffer is NUL-terminated by the API (zeroed on failure).
    // Formats and layouts are built per paint. Paints happen only on minute, media and hover changes
    // and the frames of short animations, whose morphing sizes a cache would miss anyway.
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
        format.SetTextAlignment(font.align)?;
        // A right-to-left media title starts at its column's right edge and is trimmed on its left.
        // Centred and trailing text keep the accepted left-to-right paragraph.
        if rtl {
            format.SetReadingDirection(DWRITE_READING_DIRECTION_RIGHT_TO_LEFT)?;
        }
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        // Long localized dates end in an ellipsis instead of being cut mid-glyph by the body clip.
        let ellipsis = factory.CreateEllipsisTrimmingSign(&format)?;
        let trimming = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            ..Default::default()
        };
        format.SetTrimming(&trimming, &ellipsis)?;
        let layout = factory.CreateTextLayout(&text, &format, width, height)?;
        if font.designator < 1.0 {
            for range in designator_ranges(&text) {
                layout.SetFontSize(font.size * font.designator, range)?;
            }
        }
        Ok(layout)
    }
}

/// Whether text reads right to left: its first letter is from a right-to-left script block (Hebrew,
/// Arabic, Syriac, Thaana, N'Ko and others). Digits, spaces, symbols and punctuation are skipped.
// shortcut: script blocks stand in for the Unicode Bidi_Class table; use IDWriteTextAnalyzer if a real
// title starts on the wrong side.
fn right_to_left(text: &str) -> bool {
    text.chars().find(|c| c.is_alphabetic()).is_some_and(|c| {
        matches!(
            u32::from(c),
            0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF
        )
    })
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
            context.DrawBitmap(
                &upload(context, pixels)?,
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
mod media_tests {
    use super::*;
    use crate::layout::{NOTCH, SurfaceKind};
    use crate::media::View;
    use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS;

    #[test]
    fn art_uploads_once_per_entry_and_no_art_or_a_failed_upload_releases_it() {
        let art = IconPixels {
            width: 1,
            height: 1,
            pixels: vec![0; 4],
        };
        let mut slot = None;
        let mut uploads = 0;
        let mut paint = |slot: &mut Option<(u64, u32)>, entry: Option<u64>| {
            cached(slot, entry.map(|entry| (entry, &art)), |_| {
                uploads += 1;
                Ok(uploads)
            })
            .unwrap()
        };
        // Repaints of unchanged art (progress ticks, visualizer, clock) upload nothing more.
        for _ in 0..15 {
            assert_eq!(paint(&mut slot, Some(1)), Some(1));
        }
        // A new entry uploads once; no art releases it, and its return uploads again.
        assert_eq!(paint(&mut slot, Some(2)), Some(2));
        assert_eq!(paint(&mut slot, Some(2)), Some(2));
        assert_eq!(paint(&mut slot, None), None);
        assert_eq!(slot, None);
        assert_eq!(paint(&mut slot, Some(2)), Some(3));
        // A failed upload holds nothing, so the next paint retries.
        let failed = cached(&mut slot, Some((4, &art)), |_| Err::<u32, _>(E_FAIL.into()));
        assert!(failed.is_err());
        assert_eq!(slot, None);
    }

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
        D2D_RECT_F {
            left,
            top,
            right,
            bottom,
        }
    }
    /// The media body as `Canvas::paint_notch` places it in a window-sized surface at (0, 0).
    fn body(dpi: u32, progress: f32) -> D2D_RECT_F {
        let work = Bounds {
            x: 0,
            y: 0,
            width: 3840,
            height: 2160,
        };
        let window = layout::placement(work, dpi, SurfaceKind::Notch, 0).width;
        let pixels = layout::notch_body(window, dpi, layout::notch_shape(progress, true));
        rect(
            pixels.left as f32,
            0.0,
            (pixels.left + pixels.width) as f32,
            pixels.height as f32,
        )
    }
    fn track(title: &str, artist: &str, playing: bool) -> View {
        View::Track(Track {
            title: title.into(),
            artist: artist.into(),
            playing,
            ..Track::default()
        })
    }
    /// The clock's measured width, as `notch_text` passes it.
    fn clock_width(time: &str, progress: f32, dpi: u32) -> f32 {
        let font = media_clock(motion::smooth(progress), dpi as f32 / 96.0);
        text_width(time, &font).unwrap()
    }
    /// The scene `Desktop` builds from a media view, planned as `notch_text` plans it.
    fn plan<'a>(view: &'a View, time: &'a str, progress: f32, dpi: u32) -> MediaPlan<'a> {
        focused(view, time, progress, dpi, None)
    }
    fn focused<'a>(
        view: &'a View,
        time: &'a str,
        progress: f32,
        dpi: u32,
        focus: Option<usize>,
    ) -> MediaPlan<'a> {
        played(view, time, progress, dpi, focus, None)
    }
    fn played<'a>(
        view: &'a View,
        time: &'a str,
        progress: f32,
        dpi: u32,
        focus: Option<usize>,
        played: Option<Progress>,
    ) -> MediaPlan<'a> {
        planned(view, time, progress, dpi, focus, played, None)
    }
    fn volume<'a>(
        view: &'a View,
        time: &'a str,
        progress: f32,
        dpi: u32,
        focus: Option<usize>,
        volume: Option<Level>,
    ) -> MediaPlan<'a> {
        planned(view, time, progress, dpi, focus, None, volume)
    }
    fn planned<'a>(
        view: &'a View,
        time: &'a str,
        progress: f32,
        dpi: u32,
        focus: Option<usize>,
        played: Option<Progress>,
        volume: Option<Level>,
    ) -> MediaPlan<'a> {
        let scene = NotchScene {
            time,
            date: "Tuesday",
            track: view.shown(),
            art: None,
            progress,
            dpi,
            focus,
            played,
            volume,
            bars: [0; BARS],
        };
        let clock = clock_width(time, progress, dpi);
        media_plan(&scene, scene.track.unwrap(), &body(dpi, progress), clock)
    }

    #[test]
    fn album_art_fills_the_cover_slot_and_only_the_pill_carries_a_badge() {
        let song = track("Song", "Artist", true);
        let art = IconPixels {
            width: 72,
            height: 72,
            pixels: vec![0; 72 * 72 * 4],
        };
        let planned = |progress| {
            let scene = NotchScene {
                time: "9:50 pm",
                date: "Tuesday",
                track: song.shown(),
                art: Some((1, &art)),
                progress,
                dpi: 96,
                focus: None,
                played: None,
                volume: None,
                bars: [0; BARS],
            };
            let clock = clock_width("9:50 pm", progress, 96);
            let plan = media_plan(&scene, scene.track.unwrap(), &body(96, progress), clock);
            (plan.cover, plan.art.is_some(), plan.badge)
        };
        let (cover, drawn, badge) = planned(0.0);
        // The art takes exactly the no-art slot; nothing else moves.
        assert_eq!(cover, plan(&song, "9:50 pm", 0.0, 96).cover);
        assert!(drawn);
        // An 11 badge 4 past the cover's corner (99, 5)-(119, 25), with a centred 6 glyph.
        let badge = badge.unwrap();
        assert_eq!(badge.circle.rect, rect(112.0, 18.0, 123.0, 29.0));
        assert_eq!(badge.circle.radiusX, 5.5);
        assert_eq!(badge.glyph, rect(115.0, 21.0, 121.0, 27.0));
        assert_eq!(badge.alpha, 1.0);
        let (cover, drawn, badge) = planned(1.0);
        // The panel cover, 72 at (16, 14), has no badge: its play/pause control shows the state.
        let left = body(96, 1.0).left;
        assert_eq!(cover.rect, rect(left + 16.0, 14.0, left + 88.0, 86.0));
        assert!(drawn && badge.is_none());
        assert!(plan(&song, "9:50 pm", 0.0, 96).badge.is_none());
    }
    /// Text, rectangle, size and opacity of each drawn line.
    fn lines<'a>(plan: &'a MediaPlan) -> Vec<(&'a str, D2D_RECT_F, f32, f32)> {
        plan.lines
            .iter()
            .map(|line| (line.text.as_ref(), line.rect, line.font.size, line.alpha))
            .collect()
    }

    #[test]
    fn no_player_views_draw_the_plain_clock_pill() {
        for view in [View::NoPlayer, View::Permission, View::Error] {
            assert!(view.shown().is_none());
            assert_eq!(layout::notch_shape(0.0, view.shown().is_some()), NOTCH);
        }
    }

    #[test]
    fn collapsed_playing_pill_draws_cover_slot_title_and_clock() {
        let song = track("Song", "Artist", true);
        let plan = plan(&song, "9:50 pm", 0.0, 96);
        // 262 x 30 body centred in the 440 DIP window: x 89..351.
        assert_eq!(plan.cover.rect, rect(99.0, 5.0, 119.0, 25.0));
        assert_eq!(plan.cover.radiusX, 5.0);
        assert_eq!(plan.glyph, rect(104.0, 10.0, 114.0, 20.0));
        assert!(plan.playing);
        assert_eq!(plan.dim, 1.0);
        // The title ends 8 before the clock's measured width; the artist is not laid out.
        let title_end = 337.0 - clock_width("9:50 pm", 0.0, 96) - 8.0;
        assert_eq!(
            lines(&plan),
            [
                ("9:50 pm", rect(119.0, 3.0, 337.0, 27.0), 15.0, 1.0),
                ("Song", rect(127.0, 3.0, title_end, 27.0), 12.0, 0.85),
            ]
        );
        let clock = &plan.lines[0].font;
        assert_eq!(
            (clock.weight, clock.designator, clock.align),
            (
                DWRITE_FONT_WEIGHT_BOLD,
                TIME_DESIGNATOR,
                DWRITE_TEXT_ALIGNMENT_TRAILING
            )
        );
        let title = &plan.lines[1].font;
        assert_eq!(
            (title.weight, title.align),
            (DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING)
        );
        // The pill draws no controls, even with keyboard focus on one.
        let plan = focused(&song, "9:50 pm", 0.0, 96, Some(PLAY_PAUSE));
        assert!(plan.chips.is_empty() && plan.ring.is_none());
    }

    #[test]
    fn paused_and_pending_metadata_draw_the_plain_clock_shape() {
        for view in [track("Song", "Artist", false), track("", "", true)] {
            assert!(view.shown().is_none());
            assert_eq!(layout::notch_shape(0.0, view.shown().is_some()), NOTCH);
        }
    }

    #[test]
    fn expanded_panel_places_cover_title_artist_and_clock_above_the_reserved_rows() {
        let song = track("Song", "Artist", true);
        let plan = plan(&song, "9:50 pm", 1.0, 96);
        // 360 x 120 body at x 40..400: cover 72 at (16, 14), title column from 100.
        assert_eq!(plan.cover.rect, rect(56.0, 14.0, 128.0, 86.0));
        assert_eq!(plan.cover.radiusX, 10.0);
        assert_eq!(plan.glyph, rect(80.0, 38.0, 104.0, 62.0));
        let title_end = 384.0 - clock_width("9:50 pm", 1.0, 96) - 6.0;
        assert_eq!(
            lines(&plan),
            [
                ("9:50 pm", rect(128.0, 7.0, 384.0, 31.0), 12.0, 0.60),
                ("Song", rect(140.0, 9.0, title_end, 33.0), 13.0, 1.0),
                ("Artist", rect(140.0, 28.0, title_end, 48.0), 12.0, 0.60),
            ]
        );
        assert_eq!(plan.lines[0].font.weight, DWRITE_FONT_WEIGHT_NORMAL);
        assert_eq!(plan.lines[1].font.weight, DWRITE_FONT_WEIGHT_SEMI_BOLD);
        // Text stays above the progress, controls and volume rows (from 50 DIP).
        assert!(plan.lines.iter().all(|line| line.rect.bottom <= 50.0));
    }

    #[test]
    fn expanded_panel_draws_previous_play_pause_and_next_in_the_controls_row() {
        let mut song = track("Song", "Artist", true);
        if let View::Track(track) = &mut song {
            track.controls = [false, true, true];
        }
        let panel = plan(&song, "9:50 pm", 1.0, 96);
        // Body x 40..400; controls at 100, 148 and 200 DIP from its left, 66..98 from its top.
        let drawn: Vec<_> = panel
            .chips
            .iter()
            .map(|c| (c.rect, c.glyph, c.shape, c.filled, c.alpha))
            .collect();
        assert_eq!(
            drawn,
            [
                (
                    rect(140.0, 66.0, 168.0, 98.0),
                    rect(147.0, 75.0, 161.0, 89.0),
                    Glyph::Previous,
                    false,
                    DISABLED
                ),
                (
                    rect(188.0, 66.0, 220.0, 98.0),
                    rect(198.0, 76.0, 210.0, 88.0),
                    Glyph::Pause,
                    true,
                    1.0
                ),
                (
                    rect(240.0, 66.0, 268.0, 98.0),
                    rect(247.0, 75.0, 261.0, 89.0),
                    Glyph::Next,
                    false,
                    1.0
                ),
                // Mute, disabled here: the scene has no audio endpoint.
                (
                    rect(140.0, 100.0, 164.0, 116.0),
                    rect(145.0, 101.0, 159.0, 115.0),
                    Glyph::Speaker,
                    false,
                    DISABLED
                ),
            ]
        );
        assert!(panel.ring.is_none());
        assert!(track("Song", "Artist", false).shown().is_none());
        // The controls fade in with the artist and keep their place while the body grows.
        let opening = plan(&song, "9:50 pm", 0.6, 96);
        let reveal = motion::reveal(0.6);
        assert!(reveal > 0.0 && reveal < 1.0);
        let body = body(96, 0.6);
        assert_eq!(opening.chips[1].rect.left, body.left + 148.0);
        assert_eq!(opening.chips[1].alpha, reveal);
        assert_eq!(opening.chips[0].alpha, reveal * DISABLED);
    }

    #[test]
    fn the_panel_draws_elapsed_bar_and_total_between_the_text_and_the_controls() {
        let song = track("Song", "Artist", true);
        let row = Progress {
            elapsed: 83,
            total: 296,
            filled: 41,
        };
        let panel = played(&song, "9:50 pm", 1.0, 96, None, Some(row));
        // Body x 40..400: elapsed from 100, bar 140..304 centred on y 57, total trailing at 344.
        let ([track, fill], alpha) = panel.bar.expect("a bar");
        assert_eq!(track.rect, rect(180.0, 55.0, 344.0, 59.0));
        assert_eq!(fill.rect, rect(180.0, 55.0, 221.0, 59.0));
        assert_eq!((track.radiusX, alpha), (2.0, 1.0));
        assert_eq!(
            lines(&panel)[3..],
            [
                ("1:23", rect(140.0, 49.0, 176.0, 65.0), 11.0, 0.60),
                ("4:56", rect(348.0, 49.0, 384.0, 65.0), 11.0, 0.60),
            ]
        );
        let total = &panel.lines[4].font;
        assert_eq!(total.align, DWRITE_TEXT_ALIGNMENT_TRAILING);
        // Above the controls row and inside the body at every scale.
        assert!(panel.chips.iter().all(|chip| chip.rect.top >= 65.0));
        for dpi in [144, 192] {
            let panel = played(&song, "9:50 pm", 1.0, dpi, None, Some(row));
            let body = body(dpi, 1.0);
            let ([track, _], _) = panel.bar.unwrap();
            for item in panel.lines.iter().map(|l| l.rect).chain([track.rect]) {
                assert!(item.left >= body.left && item.right <= body.right, "{dpi}");
            }
        }
        // It fades in with the controls; collapsed or without a row, nothing is drawn.
        let opening = played(&song, "9:50 pm", 0.6, 96, None, Some(row));
        assert_eq!(opening.bar.unwrap().1, motion::reveal(0.6));
        assert!(
            played(&song, "9:50 pm", 0.0, 96, None, Some(row))
                .bar
                .is_none()
        );
        let plain = plan(&song, "9:50 pm", 1.0, 96);
        assert!(plain.bar.is_none() && plain.lines.len() == 3);
    }

    #[test]
    fn keyboard_focus_rings_the_focused_control_at_any_scale() {
        let song = track("Song", "Artist", true);
        let plan = focused(&song, "9:50 pm", 1.0, 96, Some(PLAY_PAUSE));
        let ring = plan.ring.expect("a ring");
        // 2 DIP stroke centred 3 DIP outside the 32 DIP circle: a 38 DIP circle.
        assert_eq!(ring.shape.rect, rect(185.0, 63.0, 223.0, 101.0));
        assert_eq!(
            (ring.shape.radiusX, ring.stroke, ring.alpha),
            (19.0, 2.0, 1.0)
        );
        let next = focused(&song, "9:50 pm", 1.0, 96, Some(2)).ring.unwrap();
        assert_eq!(next.shape.rect, rect(237.0, 63.0, 271.0, 101.0));
        assert_eq!(next.shape.radiusX, 11.0);
        for dpi in [144, 192] {
            let scale = dpi as f32 / 96.0;
            let plan = focused(&song, "9:50 pm", 1.0, dpi, Some(0));
            let (chip, ring) = (&plan.chips[0], plan.ring.unwrap());
            assert_eq!(ring.stroke, 2.0 * scale);
            assert_eq!(chip.rect.left - ring.shape.rect.left, 3.0 * scale);
            // Every control and its ring stay inside the panel body.
            let body = body(dpi, 1.0);
            for item in plan.chips.iter().map(|c| c.rect).chain([ring.shape.rect]) {
                assert!(item.left >= body.left && item.right <= body.right, "{dpi}");
                assert!(item.top >= body.top && item.bottom <= body.bottom, "{dpi}");
            }
        }
    }

    #[test]
    fn meter_bars_sit_right_of_the_controls_bottom_aligned_and_fade_in_with_them() {
        let song = track("Song", "Artist", true);
        let meter = |progress| {
            let scene = NotchScene {
                time: "9:50 pm",
                date: "Tuesday",
                track: song.shown(),
                art: None,
                progress,
                dpi: 96,
                focus: None,
                played: None,
                volume: None,
                bars: [4, 20, 12, 8, 4],
            };
            meter_bars(&scene, &body(96, progress))
        };
        let body = body(96, 1.0);
        let (bars, alpha) = meter(1.0).unwrap();
        assert_eq!(alpha, METER_FILL);
        let lefts = bars.map(|bar| bar.rect.left - body.left);
        assert_eq!(lefts, [312.0, 319.0, 326.0, 333.0, 340.0]);
        assert_eq!(bars[4].rect.right - body.left, 344.0);
        assert!(bars.iter().all(|bar| bar.rect.bottom - body.top == 92.0));
        assert_eq!(bars[1].rect.top - body.top, 72.0);
        assert_eq!(bars[0].rect.bottom - bars[0].rect.top, 4.0);
        assert_eq!(meter(0.6).unwrap().1, METER_FILL * motion::reveal(0.6));
        assert!(meter(0.0).is_none());
    }

    #[test]
    fn volume_row_fills_to_the_shown_percent_and_dims_while_muted_or_unavailable() {
        let song = track("Song", "Artist", true);
        let level = |percent, muted| Some(Level { percent, muted });
        let panel = volume(&song, "9:50 pm", 1.0, 96, None, level(40, false));
        let bar = panel.volume.unwrap();
        // Bar x 172..384 in the body at 40; 4 DIP thick, centred in 100..116.
        assert_eq!(bar.track.rect, rect(172.0, 106.0, 384.0, 110.0));
        assert_eq!(bar.fill.rect, rect(172.0, 106.0, 172.0 + 85.0, 110.0));
        assert_eq!((bar.track_alpha, bar.fill_alpha), (VOLUME_TRACK, 1.0));
        assert_eq!(
            (panel.chips[3].shape, panel.chips[3].alpha),
            (Glyph::Speaker, 1.0)
        );
        let muted = volume(&song, "9:50 pm", 1.0, 96, None, level(40, true));
        assert_eq!(muted.chips[3].shape, Glyph::Muted);
        assert_eq!(muted.chips[3].alpha, 1.0); // Still a live toggle.
        assert_eq!(muted.volume.unwrap().fill_alpha, MUTED_FILL);
        let unavailable = volume(&song, "9:50 pm", 1.0, 96, None, None);
        let bar = unavailable.volume.unwrap();
        assert_eq!((bar.fill.rect.right, bar.fill_alpha), (172.0, DISABLED));
        // The ring fits the mute toggle inside the body; the row is not drawn while collapsed.
        let ring = volume(&song, "9:50 pm", 1.0, 96, Some(3), level(40, false)).ring;
        assert_eq!(ring.unwrap().shape.rect, rect(137.0, 97.0, 167.0, 119.0));
        assert!(
            volume(&song, "9:50 pm", 0.0, 96, None, level(40, false))
                .volume
                .is_none()
        );
        let full = volume(&song, "9:50 pm", 1.0, 192, None, level(100, false))
            .volume
            .unwrap();
        assert_eq!(full.fill.rect.right, full.track.rect.right);
        assert!(full.track.rect.bottom <= body(192, 1.0).bottom);
    }

    #[test]
    fn empty_artist_and_title_are_not_laid_out() {
        let texts = |view: &View| -> Vec<String> {
            let plan = plan(view, "9:50 pm", 1.0, 96);
            plan.lines.iter().map(|l| l.text.to_string()).collect()
        };
        assert_eq!(texts(&track("Song", "", true)), ["9:50 pm", "Song"]);
        assert_eq!(texts(&track("", "Artist", true)), ["9:50 pm", "Artist"]);
    }

    #[test]
    fn media_rows_scale_and_stay_inside_the_body_at_100_150_and_200_percent() {
        let song = track("Song", "Artist", true);
        for dpi in [96, 144, 192] {
            let scale = dpi as f32 / 96.0;
            for progress in [0.0, 0.5, 1.0] {
                let body = body(dpi, progress);
                let open = motion::smooth(progress);
                let plan = plan(&song, "9:50 pm", progress, dpi);
                let (clock, title) = (plan.lines[0].rect, plan.lines[1].rect);
                assert_eq!(clock.right, body.right - (14.0 + 2.0 * open) * scale);
                assert_eq!(title.left, body.left + (38.0 + 62.0 * open) * scale);
                assert_eq!(plan.lines[1].font.size, (12.0 + open) * scale);
                let cover = plan.cover.rect;
                assert_eq!(cover.right - cover.left, (20.0 + 52.0 * open) * scale);
                for item in plan.lines.iter().map(|l| l.rect).chain([cover]) {
                    assert!(item.left >= body.left && item.right <= body.right, "{dpi}");
                    assert!(item.top >= body.top && item.bottom <= body.bottom, "{dpi}");
                }
            }
        }
    }

    #[test]
    fn the_title_column_ends_before_the_measured_clock_and_never_inverts() {
        let song = track("Song", "", true);
        // English, Korean, German, Tamil and Malayalam afternoon formats, then one no locale has.
        let long = "9:50 ante meridiem ante meridiem ante meridiem ante meridiem";
        for time in [
            "9:50 pm",
            "\u{c624}\u{d6c4} 9:50",
            "9:50 nachm.",
            "\u{baa}\u{bbf}\u{bb1}\u{bcd}\u{baa}\u{b95}\u{bb2}\u{bcd} 9:50",
            "\u{d09}\u{d1a}\u{d4d}\u{d1a}\u{d24}\u{d3f}\u{d30}\u{d3f}\u{d1e}\u{d4d}\u{d1e}\u{d4d} 9:50",
            long,
        ] {
            for dpi in [96, 144, 192] {
                let scale = dpi as f32 / 96.0;
                for progress in [0.0, 0.5, 1.0] {
                    let clock = clock_width(time, progress, dpi);
                    let plan = plan(&song, time, progress, dpi);
                    let title = plan.lines[1].rect;
                    assert!(title.left >= plan.cover.rect.right, "{time} {dpi}");
                    assert!(title.right >= title.left, "{time} {dpi} {progress}");
                    let gap = (8.0 - 2.0 * motion::smooth(progress)) * scale;
                    if time != long {
                        // Real formats leave a title column.
                        assert!(title.right - title.left > 40.0 * scale, "{time} {dpi}");
                        assert_eq!(title.right, plan.lines[0].rect.right - clock - gap);
                    }
                }
            }
        }
        // A clock wider than the row leaves an empty, not inverted, title column.
        let plan = plan(&song, long, 0.0, 96);
        assert_eq!(plan.lines[1].rect.right, plan.lines[1].rect.left);
    }

    fn metrics(
        text: &str,
        align: DWRITE_TEXT_ALIGNMENT,
        width: f32,
    ) -> (DWRITE_TEXT_METRICS, bool) {
        let font = Font {
            family: TEXT_FONT,
            size: 13.0,
            weight: DWRITE_FONT_WEIGHT_SEMI_BOLD,
            designator: 1.0,
            align,
        };
        let layout = text_layout(text, &font, width, 24.0).unwrap();
        let mut metrics = DWRITE_TEXT_METRICS::default();
        // SAFETY: a local layout and a valid output struct.
        unsafe {
            layout.GetMetrics(&mut metrics).unwrap();
            let rtl = layout.GetReadingDirection() == DWRITE_READING_DIRECTION_RIGHT_TO_LEFT;
            (metrics, rtl)
        }
    }

    #[test]
    fn long_titles_trim_to_one_line_within_their_column() {
        // About the panel's title column beside a short clock (mockup: 190 DIP).
        let column = 190.0;
        for long in [
            "An Extremely Long Song Title That Cannot Possibly Fit In The Notch (Remastered 2026)",
            "\u{623}\u{63a}\u{646}\u{64a}\u{629} \u{637}\u{648}\u{64a}\u{644}\u{629} \u{62c}\u{62f}\u{627}\u{64b} \u{644}\u{627} \u{62a}\u{62a}\u{633}\u{639} \u{641}\u{64a} \u{627}\u{644}\u{634}\u{631}\u{64a}\u{637} \u{627}\u{644}\u{639}\u{644}\u{648}\u{64a} \u{645}\u{647}\u{645}\u{627} \u{62d}\u{627}\u{648}\u{644}\u{646}\u{627}",
        ] {
            let (natural, _) = metrics(long, DWRITE_TEXT_ALIGNMENT_LEADING, 10_000.0);
            assert!(natural.width > column, "the sample must overflow");
            let (trimmed, _) = metrics(long, DWRITE_TEXT_ALIGNMENT_LEADING, column);
            assert_eq!(trimmed.lineCount, 1);
            assert!(trimmed.left >= 0.0 && trimmed.left + trimmed.width <= column);
        }
    }

    #[test]
    fn rtl_titles_read_right_to_left_from_the_columns_right_edge() {
        let arabic = "\u{623}\u{63a}\u{646}\u{64a}\u{629}";
        let (metrics_rtl, rtl) = metrics(arabic, DWRITE_TEXT_ALIGNMENT_LEADING, 190.0);
        assert!(rtl);
        assert!(
            metrics_rtl.left > 100.0,
            "starts at the right: {}",
            metrics_rtl.left
        );
        let (metrics_ltr, rtl) = metrics("Song", DWRITE_TEXT_ALIGNMENT_LEADING, 190.0);
        assert!(!rtl);
        assert!(metrics_ltr.left < 1.0);
        // The accepted centred date and trailing clock keep their left-to-right paragraph.
        for align in [DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_TRAILING] {
            assert!(!metrics(arabic, align, 190.0).1);
        }
    }

    #[test]
    fn reading_direction_follows_the_first_letter() {
        assert!(right_to_left("\u{623}\u{63a}\u{646}\u{64a}\u{629} - Song"));
        assert!(right_to_left("\u{5e9}\u{5d9}\u{5e8}"));
        // Digits, spaces and symbols before the first letter are skipped.
        assert!(right_to_left(
            "2026 \u{1F600} \u{623}\u{63a}\u{646}\u{64a}\u{629}"
        ));
        assert!(!right_to_left("Song - \u{623}\u{63a}\u{646}\u{64a}\u{629}"));
        assert!(!right_to_left("\u{661}\u{662}"));
        assert!(!right_to_left(""));
    }
}

#[cfg(test)]
mod tests {
    use super::{designator_ranges, is_loss};
    use windows::Win32::Foundation::{D2DERR_RECREATE_TARGET, E_FAIL, E_OUTOFMEMORY};
    use windows::Win32::Graphics::Dxgi::{DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET};

    #[test]
    fn only_device_loss_results_trigger_a_rebuild() {
        for code in [
            DXGI_ERROR_DEVICE_REMOVED,
            DXGI_ERROR_DEVICE_RESET,
            D2DERR_RECREATE_TARGET,
        ] {
            assert!(is_loss(code));
        }
        for code in [E_FAIL, E_OUTOFMEMORY] {
            assert!(!is_loss(code));
        }
    }

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
