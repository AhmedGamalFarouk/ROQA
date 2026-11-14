use crate::{
    audio::{BARS, Level},
    dock,
    edge::{Layout, ListRow},
    eyes::Frame,
    icons::IconPixels,
    layout::{self, Bounds},
    media::{PLAY_PAUSE, Progress, Track},
    motion,
    tabs::Tab,
    theme::{Palette, Rgba, Sheen},
    timer,
};
use std::{borrow::Cow, cell::RefCell, mem::ManuallyDrop};
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::{
    Foundation::{D2DERR_RECREATE_TARGET, E_FAIL, HMODULE, HWND, POINT, RECT},
    Graphics::{
        Direct2D::{
            CLSID_D2D1Shadow,
            Common::{
                D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F,
                D2D1_COMPOSITE_MODE_SOURCE_OVER, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
                D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
            },
            D2D1_ANTIALIAS_MODE_ALIASED, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT,
            D2D1_ARC_SIZE_SMALL, D2D1_BITMAP_BRUSH_PROPERTIES1, D2D1_BITMAP_OPTIONS_NONE,
            D2D1_BITMAP_PROPERTIES1, D2D1_BRUSH_PROPERTIES, D2D1_BUFFER_PRECISION_8BPC_UNORM,
            D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT, D2D1_COLOR_SPACE_SRGB,
            D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_EXTEND_MODE_CLAMP,
            D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, D2D1_INTERPOLATION_MODE_LINEAR,
            D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_PROPERTY_TYPE_FLOAT,
            D2D1_PROPERTY_TYPE_VECTOR4, D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1_ROUNDED_RECT,
            D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION, D2D1_SHADOW_PROP_COLOR,
            D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
            D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateDevice, ID2D1Bitmap1, ID2D1Brush,
            ID2D1DeviceContext, ID2D1GradientStopCollection1, ID2D1LinearGradientBrush,
            ID2D1PathGeometry, ID2D1RadialGradientBrush, ID2D1SolidColorBrush,
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

mod glyphs;
mod tab_art;
pub use glyphs::Glyph;

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
    pub badge: dock::Badge,
    /// Hover animation progress, 0 (rest) to 1 (hovered).
    pub hover: f32,
}

pub struct DockScene<'a> {
    pub slots: Vec<DockSlot<'a>>,
    /// Which taskbar buttons are on, and each enabled one's hover progress.
    pub layout: dock::Buttons,
    pub buttons: Vec<(dock::Button, f32)>,
    /// Pointer-inside animation progress for the bar fill; keeps the M1 hover background semantic.
    pub inside: f32,
    pub dpi: u32,
    /// Attached style: notch material, flat bottom, rounded top corners.
    pub attached: bool,
    /// Pixels the whole dock is pushed down while it slides in or out (`dock::slide_offset`).
    pub slide: i32,
}

/// Transitions in flight, each 0 to 1 and 1 at rest: the page slide and fade (`dir` is +1 when it comes from the
/// right) and the power notice's entry.
#[derive(Clone, Copy)]
pub struct Fx {
    pub page: f32,
    pub dir: f32,
    pub notice: f32,
}
impl Fx {
    #[cfg(test)]
    pub const REST: Fx = Fx {
        page: 1.0,
        dir: 1.0,
        notice: 1.0,
    };
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
    /// The panel's progress row; none while collapsed or without a track duration.
    pub played: Option<Progress>,
    /// The panel's volume row; `None` without an audio endpoint draws it disabled.
    pub volume: Option<Level>,
    /// The visualizer's bar heights in whole pixels (`layout::meter_heights`).
    pub bars: [i32; BARS],
    /// The soonest countdown timer (S5, PROVISIONAL); `None` draws nothing.
    pub timer: Option<&'a timer::View>,
    /// The power pill's text (features B1), drawn in the open panel; `None` draws nothing.
    pub notice: Option<&'a str>,
    /// The file shelf's visible entries (S6, PROVISIONAL); empty draws nothing.
    pub shelf: &'a [ShelfSlot<'a>],
    pub shelf_undo: bool,
    /// The page the open panel shows. Media is the media and clock panel; the others draw their own content.
    pub tab: Tab,
    pub widgets: crate::widgets::WidgetConfig,
    /// The hanging tab handle; `None` draws none (fewer than two tabs).
    pub rail: Option<RailScene<'a>>,
    /// The mascot's frame in the pill; `None` at rest, when the pill shows only the time.
    pub eyes: Option<Frame>,
    /// Opacity of the microphone (orange) and camera (green) dots at the right of the pill and panel header.
    pub privacy: [f32; 2],
    /// The volume bar's pointer state: the thumb grows on hover and press, and a press shows the percent.
    pub grip: Grip,
    /// The volume bar's drawn fill in percent: it eases toward `volume`'s percent.
    pub volume_fill: f32,
    /// The Weather page's text and pictures; `Some` only while that page shows.
    pub weather: Option<&'a crate::weather::View>,
    /// The Prayer page's text; `Some` only while that page shows.
    pub prayer: Option<&'a crate::prayer_tab::View>,
    /// The Limits page's rows; `Some` only while that page shows.
    pub limits: Option<&'a crate::limits::View>,
    /// The page a switch is leaving, drawn fading out beside the new one while `fx.page` runs.
    pub leaving: Option<Tab>,
    pub fx: Fx,
}

/// The tab handle: its geometry, the cell under the pointer and the shown tabs in order.
pub struct RailScene<'a> {
    pub layout: layout::Rail,
    pub hover: Option<usize>,
    pub tabs: &'a [Tab],
}

/// One shelf entry: its system icon once the worker delivered it (a plain square until then), and whether
/// the last activation found the target gone.
pub struct ShelfSlot<'a> {
    pub icon: Option<&'a IconPixels>,
    /// The file name shown under the icon; empty for the add and more slots.
    pub name: &'a str,
    pub missing: bool,
    pub more: bool,
    pub add: bool,
}

/// Dock material (docs/design/roqa-visual-spec.md, colours from the theme): bar, ink-alpha hover fill, whole-pixel icon lift.
const ICON_LIFT_DIPS: f32 = 5.0;
const TEXT_FONT: PCWSTR = w!("Segoe UI Variable Text");
const DISPLAY_FONT: PCWSTR = w!("Segoe UI Variable Display");
/// The time's digits lead; "pm" and similar designators sit smaller beside them.
const TIME_DESIGNATOR: f32 = 0.72;

/// What an edge slider's pill shows; `layout` is in surface pixels.
pub struct SliderScene<'a> {
    pub dpi: u32,
    pub layout: &'a Layout,
    pub percent: u8,
    /// The button is down: the thumb grows.
    pub pressed: bool,
    pub glyph: Glyph,
    /// Muted or unsupported: the fill and icon are dimmed (the system grey text in high contrast).
    pub dim: bool,
    pub caption: &'a str,
    /// `None`: the caption is centred in the pill. `Some(right)`: a note beside it, trailing-aligned
    /// when the pill is on the right.
    pub note: Option<bool>,
}
pub struct ListScene<'a> {
    pub dpi: u32,
    pub panel: Bounds,
    pub rows: &'a [ListRow<'a>],
    pub hover: Option<usize>,
}

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
    /// The theme's colours, fixed for this canvas: a theme change rebuilds the surfaces.
    palette: Palette,
    /// The baked liquid-glass brushes, built on first paint and kept until the canvas is replaced.
    glass: RefCell<Option<(Sheen, Glass)>>,
}
impl Canvas {
    pub fn create(
        graphics: &Graphics,
        hwnd: HWND,
        bounds: Bounds,
        palette: Palette,
    ) -> Result<Self> {
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
                palette,
                glass: RefCell::default(),
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
        let p = self.palette;
        self.draw(|context, rect| {
            let body = D2D_RECT_F {
                left: rect.left + pixels.left as f32,
                top: rect.top,
                right: rect.left + (pixels.left + pixels.width) as f32,
                bottom: rect.top + pixels.height as f32,
            };
            let open = motion::smooth(scene.progress);
            if open > 0.0 && p.shadow > 0.0 {
                notch_shadow(context, &body, shape.radius * scale, scale, open * p.shadow)?;
            }
            Ok(())
        })
    }
    /// Black notch with concave top ears, a persistent morphing clock and the media track, if any.
    /// `status` is the one line a plain panel adds when media access failed.
    pub fn paint_notch(&self, scene: &NotchScene, status: Option<&str>) -> Result<()> {
        let scale = scene.dpi as f32 / 96.0;
        let shape = layout::notch_shape(scene.progress, scene.track.is_some());
        // The same whole-pixel body that layout::notch_region and notch_hit use.
        let pixels = layout::notch_body(self.width as i32, scene.dpi, shape);
        let p = self.palette;
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
            // The glass layers span the ears too, so the flared corners get the body's sheen, not a clamped edge.
            let ears = pixels.ears as f32;
            self.body(
                context,
                &D2D_RECT_F {
                    left: body.left - ears,
                    right: body.right + ears,
                    ..body
                },
                p.body,
                // A faint edge so the panel does not merge into a dark backdrop.
                Some(0.14),
                &geometry_shape(context, &outline),
            )?;
            if p.rim.is_some() {
                let radius = layout::notch_radius(pixels, shape, scene.dpi);
                glass_scrim(
                    context,
                    &body,
                    radius,
                    scale,
                    motion::reveal(scene.progress),
                    &p,
                )?;
            }
            // SAFETY: context is live between surface BeginDraw/EndDraw; the clip is popped below.
            unsafe {
                context.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                context.PushAxisAlignedClip(&body, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            }
            let art = cached(&mut self.art.borrow_mut(), scene.art, |pixels| {
                upload(context, pixels)
            });
            let drawn = art
                .and_then(|art| notch_text(context, scene, &body, scale, art.as_ref(), status, &p));
            // SAFETY: balances the PushAxisAlignedClip above on the same live context.
            unsafe { context.PopAxisAlignedClip() };
            drawn?;
            // The handle hangs below the body, so it is drawn outside the body's clip.
            match &scene.rail {
                Some(handle) => tab_art::rail(context, scene, handle, rect, &p),
                None => Ok(()),
            }
        })
    }
    /// Repaints only the visualizer slot of a settled, open media panel: the rest of the surface is
    /// kept, so a bars-only change skips the text, brushes and full-surface draw of `paint_notch`.
    pub fn paint_meter(&self, dpi: u32, bars: [i32; BARS]) -> Result<()> {
        let update = meter_update(self.width as i32, dpi);
        let left = layout::notch_body(self.width as i32, dpi, layout::notch_shape(1.0, true)).left;
        let p = self.palette;
        self.draw_within(Some(update), |context, rect| {
            let body = D2D_RECT_F {
                left: rect.left + left as f32,
                top: rect.top,
                ..rect
            };
            let slot = D2D_RECT_F {
                left: rect.left + update.left as f32,
                top: rect.top + update.top as f32,
                right: rect.left + update.right as f32,
                bottom: rect.top + update.bottom as f32,
            };
            // SAFETY: context is live between surface BeginDraw/EndDraw; brushes are scoped to this draw.
            unsafe {
                // The cleared slot sits inside the panel body: refill it with the body (the glass layers
                // placed on the whole open body, so the slot matches its surroundings), then the bars.
                if let Some(glass) = self.glass(context)? {
                    let size =
                        layout::notch_body(self.width as i32, dpi, layout::notch_shape(1.0, true));
                    glass.place(&D2D_RECT_F {
                        right: body.left + size.width as f32,
                        bottom: rect.top + size.height as f32,
                        ..body
                    });
                    context.FillRectangle(&slot, &glass.body);
                    context.FillRectangle(&slot, &glass.spec);
                } else {
                    let black = context.CreateSolidColorBrush(&color(p.body), None)?;
                    context.FillRectangle(&slot, &black);
                }
                if let Some((bars, alpha)) = meter_rects(1.0, dpi, bars, &body) {
                    let brush = context
                        .CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(alpha)), None)?;
                    for bar in &bars {
                        context.FillRoundedRectangle(bar, &brush);
                    }
                }
            }
            Ok(())
        })
    }
    /// An edge slider's pill (`edge.rs`): the level fill in the accent, the icon at the bottom and the
    /// caption (the percent while dragging, or why it does nothing).
    pub fn paint_slider(&self, scene: &SliderScene) -> Result<()> {
        let p = self.palette;
        let scale = scene.dpi as f32 / 96.0;
        self.draw(|context, rect| {
            let at = |b: Bounds| surface_rect(rect, b);
            let layout = scene.layout;
            self.panel(context, at(layout.pill), layout.pill.width as f32 / 2.0)?;
            let track = at(layout.track);
            let thickness = track.right - track.left;
            draw_track(context, &p, &rounded(track, thickness / 2.0), 0.40)?;
            let filled = (track.bottom - track.top) * f32::from(scene.percent.min(100)) / 100.0;
            if filled >= 1.0 {
                let fill = D2D_RECT_F {
                    top: track.bottom - filled,
                    ..track
                };
                let shape = rounded(fill, (thickness / 2.0).min(filled / 2.0));
                // SAFETY: live draw context between BeginDraw/EndDraw; the brush is scoped to this draw.
                unsafe {
                    let brush =
                        context.CreateSolidColorBrush(&tint(p.accent_dim(scene.dim, 1.0)), None)?;
                    context.FillRoundedRectangle(&shape, &brush);
                }
            }
            if scene.note.is_none() {
                // The thumb rides the fill's end and grows while pressed.
                let r = thickness / 2.0 + (if scene.pressed { 4.0 } else { 2.0 }) * scale;
                let (x, y) = ((track.left + track.right) / 2.0, track.bottom - filled);
                let dot = D2D_RECT_F {
                    left: x - r,
                    top: y - r,
                    right: x + r,
                    bottom: y + r,
                };
                // SAFETY: live draw context between BeginDraw/EndDraw; the brush is scoped to this draw.
                unsafe {
                    let brush =
                        context.CreateSolidColorBrush(&tint(p.ink_dim(scene.dim, 0.95)), None)?;
                    context.FillRoundedRectangle(&rounded(dot, r), &brush);
                }
            }
            let icon = at(layout.icon);
            let square = pixel_square(icon.left, icon.top, icon.right - icon.left);
            glyphs::draw(
                context,
                square,
                scene.glyph,
                tint(p.ink_dim(scene.dim, 0.90)),
            )?;
            if !scene.caption.is_empty() {
                let font = Font {
                    family: TEXT_FONT,
                    size: 12.0 * scale,
                    weight: DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    designator: 1.0,
                    align: match scene.note {
                        None => DWRITE_TEXT_ALIGNMENT_CENTER,
                        Some(false) => DWRITE_TEXT_ALIGNMENT_LEADING,
                        Some(true) => DWRITE_TEXT_ALIGNMENT_TRAILING,
                    },
                };
                let ink = tint(p.ink_dim(false, 0.90));
                draw_text(context, scene.caption, &font, at(layout.caption), ink)?;
            }
            Ok(())
        })
    }
    /// The volume slider's device list: a panel of rows, the current device marked with an accent dot
    /// and the hovered row lifted.
    pub fn paint_list(&self, scene: &ListScene) -> Result<()> {
        let p = self.palette;
        let scale = scene.dpi as f32 / 96.0;
        self.draw(|context, rect| {
            self.panel(context, surface_rect(rect, scene.panel), 12.0 * scale)?;
            let font = Font {
                family: TEXT_FONT,
                size: 13.0 * scale,
                weight: DWRITE_FONT_WEIGHT_NORMAL,
                designator: 1.0,
                align: DWRITE_TEXT_ALIGNMENT_LEADING,
            };
            let dot = 6.0 * scale;
            for (index, row) in scene.rows.iter().enumerate() {
                let cell = surface_rect(rect, row.rect);
                let (left, middle) = (cell.left + 10.0 * scale, (cell.top + cell.bottom) / 2.0);
                // SAFETY: live draw context between BeginDraw/EndDraw; brushes are scoped to this draw.
                unsafe {
                    if scene.hover == Some(index) {
                        let brush = context.CreateSolidColorBrush(&tint(p.track(0.16)), None)?;
                        context.FillRoundedRectangle(&rounded(cell, 8.0 * scale), &brush);
                    }
                    if row.current {
                        let brush =
                            context.CreateSolidColorBrush(&tint(p.accent_dim(false, 1.0)), None)?;
                        let shape = D2D_RECT_F {
                            left,
                            top: middle - dot / 2.0,
                            right: left + dot,
                            bottom: middle + dot / 2.0,
                        };
                        context.FillRoundedRectangle(&rounded(shape, dot / 2.0), &brush);
                    }
                }
                let mut text = D2D_RECT_F {
                    left: left + dot + 8.0 * scale,
                    right: cell.right - 8.0 * scale,
                    ..cell
                };
                if let Some(pixels) = row.icon {
                    let side = 16.0 * scale;
                    let frame = D2D_RECT_F {
                        left,
                        top: middle - side / 2.0,
                        right: left + side,
                        bottom: middle + side / 2.0,
                    };
                    // SAFETY: live draw context between BeginDraw/EndDraw; the bitmap is scoped to this draw.
                    unsafe {
                        context.DrawBitmap(
                            &upload(context, pixels)?,
                            Some(&frame),
                            1.0,
                            D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                            None,
                            None,
                        );
                    }
                    text.left = left + side + 8.0 * scale;
                }
                draw_text(context, row.name, &font, text, tint(p.ink_dim(false, 0.90)))?;
            }
            Ok(())
        })
    }
    pub fn paint_dock(&self, scene: &DockScene) -> Result<()> {
        let p = self.palette;
        self.draw(|context, rect| {
            // SAFETY: context is live between surface BeginDraw/EndDraw; `draw_within` restores identity.
            unsafe { context.SetTransform(&Matrix3x2::translation(0.0, scene.slide as f32)) };
            let px = |dips: i32| dock::px(dips, scene.dpi) as f32;
            let radius = px(dock::RADIUS_DIPS);
            if scene.attached {
                let bounds = layout::Bounds {
                    x: 0,
                    y: 0,
                    width: self.width as i32,
                    height: self.height as i32,
                };
                let corner = layout::attached_radius(bounds, scene.dpi);
                let outline = attached_outline(context, &rect, corner)?;
                self.body(
                    context,
                    &rect,
                    p.body,
                    Some(0.10),
                    &geometry_shape(context, &outline),
                )?;
                return paint_slots(context, &rect, self.width as i32, scene, &px, &p);
            }
            // A one-physical-pixel inner hairline separates the black bar from dark wallpapers.
            self.rounded_body(context, rect, radius, p.dock_fill(scene.inside))?;
            paint_slots(context, &rect, self.width as i32, scene, &px, &p)
        })
    }
    /// The baked liquid-glass brushes for this palette, or `None` in the other themes. Built once per
    /// canvas (the theme is fixed for it) and shared by every later paint.
    #[inline(never)]
    fn glass(&self, context: &ID2D1DeviceContext) -> Result<Option<Glass>> {
        let Some(sheen) = self.palette.sheen else {
            return Ok(None);
        };
        baked(&mut self.glass.borrow_mut(), sheen, || {
            bake(context, &self.palette, &sheen)
        })
        .map(Some)
    }
    /// One body: the flat fill with the rim (or `hairline` alpha, if any), or the baked glass layers placed
    /// on `span`. `shape(brush, None)` fills the shape, `shape(brush, Some(width))` strokes its outline.
    #[inline(never)]
    fn body(
        &self,
        context: &ID2D1DeviceContext,
        span: &D2D_RECT_F,
        flat: Rgba,
        hairline: Option<f32>,
        shape: &dyn Fn(&ID2D1Brush, Option<f32>),
    ) -> Result<()> {
        let p = &self.palette;
        if let Some(glass) = self.glass(context)? {
            glass.place(span);
            shape(&glass.body, None);
            shape(&glass.spec, None);
            shape(&glass.edge, Some(1.0));
            shape(&glass.rim, Some(1.0));
            return Ok(());
        }
        // SAFETY: context is live between surface BeginDraw/EndDraw; brushes are scoped to this call.
        unsafe {
            shape(
                &context.CreateSolidColorBrush(&color(flat), None)?.into(),
                None,
            );
            match (p.rim, hairline) {
                (Some(rim), _) => {
                    let edge = context.CreateSolidColorBrush(&rgba(0, 0, 0, GLASS_EDGE), None)?;
                    shape(&edge.into(), Some(1.0));
                    shape(
                        &rim_brush(context, p.ink, rim, (span.top, span.bottom))?.into(),
                        Some(1.0),
                    )
                }
                (None, Some(alpha)) => shape(
                    &context
                        .CreateSolidColorBrush(&with_alpha(p.ink, p.edge_alpha(alpha)), None)?
                        .into(),
                    Some(1.0),
                ),
                (None, None) => {}
            }
        }
        Ok(())
    }
    /// A rounded body in the theme's fill with the hairline (or glass rim) the dock bar has.
    #[inline(never)]
    fn rounded_body(
        &self,
        context: &ID2D1DeviceContext,
        rect: D2D_RECT_F,
        radius: f32,
        flat: Rgba,
    ) -> Result<()> {
        let edge = D2D_RECT_F {
            left: rect.left + 0.5,
            top: rect.top + 0.5,
            right: rect.right - 0.5,
            bottom: rect.bottom - 0.5,
        };
        self.body(context, &rect, flat, Some(0.10), &|brush, width| {
            // SAFETY: context is live between surface BeginDraw/EndDraw.
            unsafe {
                match width {
                    None => context.FillRoundedRectangle(&rounded(rect, radius), brush),
                    Some(width) => context.DrawRoundedRectangle(
                        &rounded(edge, radius - 0.5),
                        brush,
                        width,
                        None,
                    ),
                }
            }
        })
    }
    fn panel(&self, context: &ID2D1DeviceContext, rect: D2D_RECT_F, radius: f32) -> Result<()> {
        self.rounded_body(context, rect, radius, self.palette.body)
    }
    fn draw(
        &self,
        content: impl FnOnce(&ID2D1DeviceContext, D2D_RECT_F) -> Result<()>,
    ) -> Result<()> {
        self.draw_within(None, content)
    }
    /// Draws into the whole surface, or only `update` (surface pixels; the rest is kept). `content`
    /// gets the rectangle of the whole surface in the context's coordinates either way.
    fn draw_within(
        &self,
        update: Option<RECT>,
        content: impl FnOnce(&ID2D1DeviceContext, D2D_RECT_F) -> Result<()>,
    ) -> Result<()> {
        let mut offset = POINT::default();
        // SAFETY: surface has no outstanding draw; context is scoped to this draw and EndDraw always runs.
        unsafe {
            let area = update.as_ref().map(|update| update as *const RECT);
            let context: ID2D1DeviceContext = self.surface.BeginDraw(area, &mut offset)?;
            context.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            // `offset` is where the update rectangle's top left lands (the surface's, with none).
            let origin = update.map_or((0, 0), |update| (update.left, update.top));
            let rect = D2D_RECT_F {
                left: (offset.x - origin.0) as f32,
                top: (offset.y - origin.1) as f32,
                right: (offset.x - origin.0) as f32 + self.width as f32,
                bottom: (offset.y - origin.1) as f32 + self.height as f32,
            };
            // Shapes have transparent surroundings; clear only this draw's update rectangle.
            let cleared = update.map_or(rect, |update| D2D_RECT_F {
                left: offset.x as f32,
                top: offset.y as f32,
                right: (offset.x + update.right - update.left) as f32,
                bottom: (offset.y + update.bottom - update.top) as f32,
            });
            context.PushAxisAlignedClip(&cleared, D2D1_ANTIALIAS_MODE_ALIASED);
            context.Clear(Some(&rgba(0, 0, 0, 0.0)));
            context.PopAxisAlignedClip();
            let drawing = content(&context, rect);
            context.SetTransform(&Matrix3x2::identity());
            let ended = self.surface.EndDraw();
            drawing?;
            ended?;
            self.graphics.composition.Commit()?;
        }
        // Every notch, shadow, meter-slot and dock paint ends here, so this is the one paint count.
        crate::diagnostics::PAINTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
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

/// Soft shadow under the opening notch, fading in with it (the flush pill casts none). `strength` is the
/// open progress times the theme's shadow (Classic 0.9).
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
    let color: Vec<u8> = [0.0f32, 0.0, 0.0, strength]
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
    status: Option<&str>,
    p: &Palette,
) -> Result<()> {
    timer_text(context, scene, body, scale, p)?;
    notice_text(context, scene, body, scale, p)?;
    if let Some(frame) = scene.eyes {
        tab_art::draw_eyes(context, scene, frame, body, scale, p)?;
    }
    tab_art::draw_privacy(context, scene, body, scale)?;
    if scene.tab == Tab::Media {
        media_page(context, scene, body, scale, art, status, p)?;
    } else {
        // Another page: the pill's content fades out as the page fades in.
        let reveal = motion::reveal(scene.progress);
        tab_art::faded(context, 1.0 - reveal, || {
            media_page(context, scene, body, scale, art, status, p)
        })?;
        if let Some(old) = scene.leaving.filter(|&old| old != Tab::Media) {
            tab_art::faded(context, 1.0 - scene.fx.page, || {
                tab_art::page(context, scene, (old, true), body, scale, reveal, p)
            })?;
        }
        tab_art::faded(context, scene.fx.page, || {
            tab_art::page(context, scene, (scene.tab, false), body, scale, reveal, p)
        })?;
        // SAFETY: the page may have left a slide transform on the live context.
        unsafe { context.SetTransform(&Matrix3x2::identity()) };
    }
    settings_button(context, scene, body, p)
}

/// The cog in the open panel's bottom-right corner (`layout::gear`): a click opens Settings. It fades in with
/// the panel's controls.
fn settings_button(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    p: &Palette,
) -> Result<()> {
    let reveal = motion::reveal(scene.progress);
    if reveal <= 0.0 {
        return Ok(());
    }
    let gear = layout::gear(scene.dpi, scene.track.is_some());
    let square = pixel_square(
        body.left + gear.x as f32,
        body.top + gear.y as f32,
        gear.width as f32,
    );
    let scale = scene.dpi as f32 / 96.0;
    let background = D2D_RECT_F {
        left: square.left - 4.0 * scale,
        top: square.top - 4.0 * scale,
        right: square.right + 4.0 * scale,
        bottom: square.bottom + 4.0 * scale,
    };
    // SAFETY: the brush is local to the live drawing context.
    unsafe {
        let brush = context.CreateSolidColorBrush(&with_alpha(p.ink, 0.12 * reveal), None)?;
        context.FillRoundedRectangle(&rounded(background, 8.0 * scale), &brush);
    }
    glyphs::draw(
        context,
        square,
        Glyph::Gear,
        with_alpha(p.ink, p.ink_alpha(0.9 * reveal)),
    )
}

/// The Media page, and the pill every page starts from: the media track or the plain clock, date and status.
fn media_page(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    art: Option<&ID2D1Bitmap1>,
    status: Option<&str>,
    p: &Palette,
) -> Result<()> {
    let status = scene
        .notice
        .is_none()
        .then(|| status.unwrap_or("Nothing playing"));
    if let Some(track) = scene.track {
        let font = media_clock(motion::smooth(scene.progress), scale);
        let clock = text_width(scene.time, &font)?;
        return media_text(context, &media_plan(scene, track, body, clock), art, p);
    }
    let open = motion::smooth(scene.progress);
    let cue = (1.0 - motion::reveal(scene.progress)) * (1.0 - tab_art::eyes_shown(scene));
    if cue > 0.0 {
        glyphs::draw(
            context,
            pixel_square(
                body.right - 29.0 * scale,
                body.top + 8.0 * scale,
                14.0 * scale,
            ),
            Glyph::Expand,
            with_alpha(p.ink, p.ink_alpha(0.68 * cue)),
        )?;
    }
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
        with_alpha(p.ink, p.ink_alpha(1.0 - tab_art::eyes_shown(scene))),
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
        with_alpha(p.ink, p.ink_alpha(0.60 * date)),
    )?;
    // PROVISIONAL: why there is no media notch, under the date, 11 DIP at the date's opacity.
    match status {
        Some(line) => draw_text(
            context,
            line,
            &Font {
                size: 11.0 * scale,
                ..font
            },
            row(72.0, 6.0),
            with_alpha(p.ink, p.ink_alpha(0.60 * date)),
        ),
        None => Ok(()),
    }
}

/// PROVISIONAL (S5): the plain pill's whole minutes left on the soonest timer, 12 DIP white 85 %, fading out
/// as the panel opens. The media pill has no room and draws none; the countdown itself is the Timer page's
/// (`tab_art::page`). Slot: `layout::timer_pill`.
fn timer_text(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let reveal = motion::reveal(scene.progress);
    let Some(view) = scene
        .timer
        .filter(|_| scene.track.is_none() && reveal < 1.0)
    else {
        return Ok(());
    };
    let slot = layout::timer_pill(scene.dpi);
    let left = body.left + slot.x as f32;
    let top = body.top + slot.y as f32;
    let font = Font {
        family: TEXT_FONT,
        size: 12.0 * scale,
        weight: DWRITE_FONT_WEIGHT_NORMAL,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_TRAILING,
    };
    let rect = D2D_RECT_F {
        left,
        top,
        right: left + slot.width as f32,
        bottom: top + slot.height as f32,
    };
    draw_text(
        context,
        &view.pill,
        &font,
        rect,
        with_alpha(p.ink, p.ink_alpha(0.85 * (1.0 - reveal))),
    )
}

/// PROVISIONAL (B1): the power pill's text in 11 DIP white 85 %, fading in with the panel; it replaces the
/// plain panel's status line and shows on every page. Slot: `layout::power_notice`.
fn notice_text(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let reveal = motion::reveal(scene.progress);
    let Some(text) = scene.notice.filter(|_| reveal > 0.0) else {
        return Ok(());
    };
    let slot = layout::power_notice(scene.track.is_some(), scene.dpi);
    let left = body.left + slot.x as f32;
    // The notice rises 5 DIP into place as it fades in.
    let top = body.top + slot.y as f32 + (1.0 - scene.fx.notice) * 5.0 * scale;
    let font = Font {
        family: TEXT_FONT,
        size: 12.0 * scale,
        weight: DWRITE_FONT_WEIGHT_NORMAL,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_CENTER,
    };
    let rect = D2D_RECT_F {
        left,
        top,
        right: left + slot.width as f32,
        bottom: top + slot.height as f32,
    };
    draw_text(
        context,
        text,
        &font,
        rect,
        with_alpha(p.ink, p.ink_alpha(0.85 * reveal * scene.fx.notice)),
    )
}

/// Side of a shelf icon inside its slot, in DIPs.
const SHELF_ICON: f32 = 20.0;

/// PROVISIONAL (S6): the shelf's entries in their `layout::shelf_slot` cells, faded in with the panel. A
/// missing target is dimmed like a disabled control. The tiny icon bitmaps are uploaded per paint, which only
/// an open panel does.
// shortcut: up to `shelf::VISIBLE` bitmap uploads per full paint of an open panel; cache them per canvas
// like the art if a trace shows it.
fn shelf_row(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let reveal = motion::reveal(scene.progress);
    if reveal <= 0.0 {
        return Ok(());
    }
    let media = scene.track.is_some();
    for (index, slot) in scene.shelf.iter().enumerate() {
        let cell = layout::shelf_slot(media, index, scene.dpi);
        let side = (SHELF_ICON * scale).round();
        let inset = ((cell.width as f32 - side) / 2.0).round();
        let square = pixel_square(
            body.left + (cell.x as f32 + inset),
            body.top + (cell.y as f32 + 2.0 * scale),
            side,
        );
        let alpha = reveal * if slot.missing { DISABLED } else { 1.0 };
        let shape = rounded(square, 3.0 * scale);
        let ink = |alpha: f32| with_alpha(p.ink, p.ink_alpha(alpha));
        let name = match (slot.add, slot.more) {
            (true, _) => "Add",
            (_, true) => "More",
            _ => slot.name,
        };
        let font = Font {
            family: TEXT_FONT,
            size: 10.0 * scale,
            weight: DWRITE_FONT_WEIGHT_NORMAL,
            designator: 1.0,
            align: DWRITE_TEXT_ALIGNMENT_CENTER,
        };
        let (left, top) = (body.left + cell.x as f32, body.top + cell.y as f32);
        let line = D2D_RECT_F {
            top: square.bottom + 2.0 * scale,
            left: left + 2.0 * scale,
            right: left + cell.width as f32 - 2.0 * scale,
            bottom: top + cell.height as f32,
        };
        draw_text(context, name, &font, line, ink(0.8 * alpha))?;
        if slot.add || slot.more {
            let glyph = if slot.add { Glyph::Add } else { Glyph::More };
            glyphs::draw(context, square, glyph, ink(reveal))?;
        } else if let Some(icon) = slot.icon {
            let bitmap = upload(context, icon)?;
            fill_art(context, &shape, icon.width, &bitmap, alpha)?;
        } else {
            // No icon yet, or the shell had none: a file glyph instead of an empty square.
            glyphs::draw(context, square, Glyph::File, ink(0.6 * alpha))?;
        }
    }
    Ok(())
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
    /// Muted or without an endpoint: high contrast draws the fill in the system grey-text colour.
    dim: bool,
    /// The thumb at the fill's end: centre x, centre y and radius, in pixels.
    thumb: (f32, f32, f32),
    /// The percent readout above the thumb while pressed.
    readout: Option<(String, D2D_RECT_F)>,
}
/// The visualizer's bars: secondary, decorative content, white 60 % like the artist.
const METER_FILL: f32 = 0.60;
/// The bar's drawn thickness in DIPs, centred in its hit box.
const VOLUME_THICKNESS: f32 = 6.0;
/// The thumb's radius at rest, under the pointer and while pressed, in DIPs.
const THUMB: [f32; 3] = [4.5, 6.0, 7.5];
/// The bar's track: white 40 % is about 3.7 : 1 on black, the non-text contrast a target needs.
const VOLUME_TRACK: f32 = 0.40;
/// The fill while muted: white 60 % over the track is about 3.2 : 1 against it, and dimmer than the
/// audible white fill.
const MUTED_FILL: f32 = 0.60;

/// One media control: a line glyph, with a soft accent wash for play/pause. A control
/// the player does not accept now is drawn at `DISABLED` opacity.
struct Chip {
    rect: D2D_RECT_F,
    glyph: D2D_RECT_F,
    shape: Glyph,
    filled: bool,
    alpha: f32,
    /// The player does not accept it now: high contrast draws it in the system grey-text colour.
    disabled: bool,
}
/// Opacity of a control the player does not accept now.
const DISABLED: f32 = 0.30;

/// What the pointer is doing on the volume bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grip {
    pub hover: bool,
    pub drag: bool,
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
    // The title makes way while the mascot's eyes are in its lane.
    let title_fade = 1.0 - tab_art::eyes_shown(scene);
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
            alpha: at(0.85, 1.0) * dim * title_fade,
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
        volume: volume_bar(scene, body),
        meter: meter_bars(scene, body),
    }
}

/// The visualizer slot in surface pixels for a surface `width` wide, with the open panel's body.
fn meter_update(width: i32, dpi: u32) -> RECT {
    let left = layout::notch_body(width, dpi, layout::notch_shape(1.0, true)).left;
    let (slot, _) = layout::meter(dpi);
    RECT {
        left: left + slot.x,
        top: slot.y,
        right: left + slot.x + slot.width,
        bottom: slot.y + slot.height,
    }
}

/// The visualizer right of the controls row, placed and revealed like it: fully rounded bars spread
/// across the slot (`layout::meter`), bottom-aligned at the scene's heights.
fn meter_bars(scene: &NotchScene, body: &D2D_RECT_F) -> Option<([D2D1_ROUNDED_RECT; BARS], f32)> {
    meter_rects(scene.progress, scene.dpi, scene.bars, body)
}

/// `meter_bars` for bar `heights` at an animation `progress`, without a whole scene.
fn meter_rects(
    progress: f32,
    dpi: u32,
    heights: [i32; BARS],
    body: &D2D_RECT_F,
) -> Option<([D2D1_ROUNDED_RECT; BARS], f32)> {
    let reveal = motion::reveal(progress);
    if reveal <= 0.0 {
        return None;
    }
    let (slot, bar) = layout::meter(dpi);
    let bottom = body.top + (slot.y + slot.height) as f32;
    let bars = std::array::from_fn(|index| {
        let left =
            body.left + (slot.x + index as i32 * (slot.width - bar) / (BARS as i32 - 1)) as f32;
        let rect = D2D_RECT_F {
            left,
            top: bottom - heights[index] as f32,
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
    let radius = THUMB[usize::from(scene.grip.hover) + usize::from(scene.grip.drag)]
        * scene.dpi as f32
        / 96.0;
    let filled = (bar.width as f32 * scene.volume_fill.clamp(0.0, 100.0) / 100.0).round();
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
        dim: scene.volume.is_none_or(|level| level.muted),
        thumb: (left + filled, top + thickness / 2.0, radius),
        readout: (scene.grip.drag && scene.volume.is_some()).then(|| {
            let scale = scene.dpi as f32 / 96.0;
            let middle = left + filled;
            let bottom = top + thickness / 2.0 - radius - 2.0 * scale;
            (
                format!("{percent}%"),
                D2D_RECT_F {
                    left: middle - 22.0 * scale,
                    top: bottom - 14.0 * scale,
                    right: middle + 22.0 * scale,
                    bottom,
                },
            )
        }),
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
    let speaker = scene.volume.map_or(Glyph::Speaker, |level| {
        Glyph::speaker(level.percent, level.muted)
    });
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
            // A 16 DIP glyph inside the circle, 18 DIP on its own (the 24-unit icon grid has its own margin).
            let size = (if filled { 16.0 } else { 18.0 } * scale)
                .round()
                .min(rect.right - rect.left)
                .min(rect.bottom - rect.top);
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
                disabled: !enabled[index],
            }
        })
        .collect()
}

fn media_text(
    context: &ID2D1DeviceContext,
    plan: &MediaPlan,
    bitmap: Option<&ID2D1Bitmap1>,
    p: &Palette,
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
            let slot = context.CreateSolidColorBrush(&tint(p.track(COVER_FILL)), None)?;
            context.FillRoundedRectangle(&plan.cover, &slot);
        }
        glyphs::draw(
            context,
            plan.glyph,
            state,
            with_alpha(p.ink, p.ink_alpha(plan.dim)),
        )?;
    }
    if let Some(badge) = &plan.badge {
        // SAFETY: as above.
        unsafe {
            let circle = context.CreateSolidColorBrush(&with_alpha(p.on_ink, badge.alpha), None)?;
            context.FillRoundedRectangle(&badge.circle, &circle);
        }
        let color = with_alpha(p.ink, p.ink_alpha(plan.dim * badge.alpha));
        glyphs::draw(context, badge.glyph, state, color)?;
    }
    if let Some(([track, fill], alpha)) = &plan.bar {
        // SAFETY: as above.
        unsafe {
            draw_track(context, p, track, COVER_FILL * alpha)?;
            if fill.rect.right > fill.rect.left {
                let brush = context
                    .CreateSolidColorBrush(&with_alpha(p.accent, p.ink_alpha(*alpha)), None)?;
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
            with_alpha(p.ink, p.ink_alpha(line.alpha)),
        )?;
    }
    for chip in &plan.chips {
        let color = tint(p.ink_dim(chip.disabled, chip.alpha));
        if chip.filled {
            let radius = (chip.rect.bottom - chip.rect.top) / 2.0;
            // SAFETY: as above.
            unsafe {
                let circle = context.CreateSolidColorBrush(
                    &with_alpha(p.accent, p.ink_alpha(0.22 * chip.alpha)),
                    None,
                )?;
                context.FillRoundedRectangle(&rounded(chip.rect, radius), &circle);
            }
            glyphs::draw(
                context,
                chip.glyph,
                chip.shape,
                with_alpha(glyphs::active_ink(p), p.ink_alpha(chip.alpha)),
            )?;
        } else {
            glyphs::draw(context, chip.glyph, chip.shape, color)?;
        }
    }
    if let Some(bar) = &plan.volume {
        // SAFETY: as above.
        unsafe {
            draw_track(context, p, &bar.track, bar.track_alpha)?;
            let fill = context
                .CreateSolidColorBrush(&tint(p.accent_dim(bar.dim, bar.fill_alpha)), None)?;
            context.FillRoundedRectangle(&bar.fill, &fill);
            let (x, y, r) = bar.thumb;
            let thumb = context
                .CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(bar.fill_alpha)), None)?;
            let dot = D2D_RECT_F {
                left: x - r,
                top: y - r,
                right: x + r,
                bottom: y + r,
            };
            context.FillRoundedRectangle(&rounded(dot, r), &thumb);
        }
        if let Some((text, rect)) = &bar.readout {
            let font = Font {
                family: TEXT_FONT,
                size: (rect.bottom - rect.top) * 0.85,
                weight: DWRITE_FONT_WEIGHT_SEMI_BOLD,
                designator: 1.0,
                align: DWRITE_TEXT_ALIGNMENT_CENTER,
            };
            draw_text(
                context,
                text,
                &font,
                *rect,
                with_alpha(p.ink, p.ink_alpha(0.90)),
            )?;
        }
    }
    if let Some((bars, alpha)) = &plan.meter {
        // SAFETY: as above.
        unsafe {
            let brush =
                context.CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(*alpha)), None)?;
            for bar in bars {
                context.FillRoundedRectangle(bar, &brush);
            }
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
    // Formats and layouts are built per paint. A bars-only change repaints the meter slot alone
    // (`Canvas::paint_meter`), so full paints are minute, media, progress-tick, volume and hover
    // changes and animation frames, whose morphing sizes a cache would miss anyway.
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
/// "9:50 pm" or "ì˜¤í›„ " in "ì˜¤í›„ 10:00". Any script's digits count, so the user-locale format is kept.
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

/// The dock's slots and taskbar buttons inside `rect` (either style), in the dock's reading direction.
fn paint_slots(
    context: &ID2D1DeviceContext,
    rect: &D2D_RECT_F,
    width: i32,
    scene: &DockScene,
    px: &dyn Fn(i32) -> f32,
    p: &Palette,
) -> Result<()> {
    let visible = dock::visible_slots(scene.layout, width, scene.dpi, scene.slots.len());
    let cell = |hit| {
        let (left, cell) = dock::span(scene.layout, width, scene.dpi, hit);
        D2D_RECT_F {
            left: rect.left + left as f32,
            top: rect.top,
            right: rect.left + (left + cell) as f32,
            bottom: rect.bottom,
        }
    };
    for (index, slot) in scene.slots.iter().take(visible).enumerate() {
        draw_slot(
            context,
            &cell(dock::Hit::Slot(index)),
            slot,
            scene.dpi,
            px,
            p,
        )?;
    }
    for &(button, hover) in &scene.buttons {
        draw_button(
            context,
            &cell(dock::Hit::Button(button)),
            button,
            hover,
            px,
            p,
        )?;
    }
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brushes are scoped to this draw.
    unsafe {
        // Hairlines between the button groups and the slots.
        let line = context.CreateSolidColorBrush(&tint(p.track(0.22)), None)?;
        for x in dock::separators(scene.layout, width, scene.dpi)
            .into_iter()
            .flatten()
        {
            let at = rect.left + x as f32;
            let hairline = D2D_RECT_F {
                left: at - 0.5,
                top: rect.top + px(20),
                right: at + 0.5,
                bottom: rect.bottom - px(20),
            };
            context.FillRectangle(&hairline, &line);
        }
    }
    Ok(())
}

/// One taskbar button: the hover chip, then its glyph.
fn draw_button(
    context: &ID2D1DeviceContext,
    cell: &D2D_RECT_F,
    button: dock::Button,
    hover: f32,
    px: &dyn Fn(i32) -> f32,
    p: &Palette,
) -> Result<()> {
    let centre = (
        (cell.left + cell.right) / 2.0,
        (cell.top + cell.bottom) / 2.0,
    );
    if button == dock::Button::Desktop {
        return glyph_desktop(context, cell, hover, px, p);
    }
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brushes are scoped to this draw.
    unsafe {
        if hover > 0.0 {
            let (half_w, half_h) = ((cell.right - cell.left) / 2.0 - px(2), px(18));
            let chip = D2D_RECT_F {
                left: centre.0 - half_w,
                top: centre.1 - half_h,
                right: centre.0 + half_w,
                bottom: centre.1 + half_h,
            };
            let brush = context.CreateSolidColorBrush(&tint(p.track(0.12 * hover)), None)?;
            context.FillRoundedRectangle(&rounded(chip, px(10)), &brush);
        }
    }
    let side = px(22);
    let square = pixel_square(centre.0 - side / 2.0, centre.1 - side / 2.0, side);
    let shape = match button {
        dock::Button::Start => Glyph::Start,
        dock::Button::Search => Glyph::Search,
        dock::Button::TaskView => Glyph::TaskView,
        _ => Glyph::Chevron,
    };
    glyphs::draw(context, square, shape, with_alpha(p.glyph, 1.0))
}

/// The monitor marks the show-desktop control at the dock's far end.
fn glyph_desktop(
    context: &ID2D1DeviceContext,
    cell: &D2D_RECT_F,
    hover: f32,
    px: &dyn Fn(i32) -> f32,
    p: &Palette,
) -> Result<()> {
    let side = px(18).min(cell.right - cell.left);
    let square = pixel_square(
        (cell.left + cell.right - side) / 2.0,
        (cell.top + cell.bottom - side) / 2.0,
        side,
    );
    glyphs::draw(
        context,
        square,
        Glyph::Desktop,
        with_alpha(p.glyph, p.ink_alpha(0.72 + 0.28 * hover)),
    )
}

/// The notch outline turned over: rounded top corners, flat bottom flush with the surface's bottom edge.
fn attached_outline(
    context: &ID2D1DeviceContext,
    body: &D2D_RECT_F,
    radius: f32,
) -> Result<ID2D1PathGeometry> {
    let (left, top, right, bottom) = (body.left, body.top, body.right, body.bottom);
    let point = |x: f32, y: f32| Vector2 { X: x, Y: y };
    let arc = |x: f32, y: f32| D2D1_ARC_SEGMENT {
        point: point(x, y),
        size: D2D_SIZE_F {
            width: radius,
            height: radius,
        },
        rotationAngle: 0.0,
        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
        arcSize: D2D1_ARC_SIZE_SMALL,
    };
    // SAFETY: the factory, geometry and sink are local COM objects; the sink is closed before use.
    unsafe {
        let path = context.GetFactory()?.CreatePathGeometry()?;
        let sink = path.Open()?;
        sink.BeginFigure(point(left, bottom), D2D1_FIGURE_BEGIN_FILLED);
        sink.AddLine(point(left, top + radius));
        sink.AddArc(&arc(left + radius, top));
        sink.AddLine(point(right - radius, top));
        sink.AddArc(&arc(right, top + radius));
        sink.AddLine(point(right, bottom));
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        Ok(path)
    }
}

/// The hovered icon's side and lift in device pixels: it grows by up to 10 % and rises `ICON_LIFT_DIPS`. Both
/// move smoothly while the hover animates (whole pixels would show the 5 px of travel in a handful of steps
/// bunched in the middle of the curve) and snap to whole pixels at rest, which keeps shell icon bitmaps crisp.
fn icon_metrics(hover: f32, icon: f32, dpi: u32) -> (f32, f32) {
    let settle = |value: f32| {
        if hover == 0.0 || hover == 1.0 {
            value.round()
        } else {
            value
        }
    };
    (
        settle(icon * (1.0 + 0.1 * hover)),
        settle(ICON_LIFT_DIPS * dpi as f32 / 96.0 * hover),
    )
}

fn draw_slot(
    context: &ID2D1DeviceContext,
    cell: &D2D_RECT_F,
    slot: &DockSlot,
    dpi: u32,
    px: &dyn Fn(i32) -> f32,
    p: &Palette,
) -> Result<()> {
    let (size, lift) = icon_metrics(slot.hover, px(dock::ICON_DIPS), dpi);
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
            let brush = context.CreateSolidColorBrush(&tint(p.track(0.12 * slot.hover)), None)?;
            context.FillRoundedRectangle(&rounded(highlight, px(12)), &brush);
        }
        if slot.picker {
            glyphs::draw(context, frame, Glyph::Apps, with_alpha(p.glyph, 1.0))?;
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
            let brush = context.CreateSolidColorBrush(&color(p.tile), None)?;
            context.FillRoundedRectangle(&rounded(frame, px(10)), &brush);
        }
        if slot.badge != dock::Badge::None {
            let label = match slot.badge {
                dock::Badge::Count(n) if n >= 100 => Some("~99+".to_owned()),
                dock::Badge::Count(n) => Some(format!("~{n}")),
                _ => None,
            };
            let width = px(match label.as_deref() {
                Some("~99+") => 30,
                Some(s) if s.len() > 2 => 24,
                Some(_) => 18,
                None => 9,
            });
            let height = if label.is_some() { px(16) } else { px(9) };
            let badge = D2D_RECT_F {
                left: frame.right - width / 2.0,
                top: frame.top - height / 4.0,
                right: frame.right + width / 2.0,
                bottom: frame.top + height * 0.75,
            };
            let brush = context.CreateSolidColorBrush(&rgba(214, 31, 41, 1.0), None)?;
            context.FillRoundedRectangle(&rounded(badge, height / 2.0), &brush);
            if let Some(label) = label {
                let font = Font {
                    family: TEXT_FONT,
                    size: px(10),
                    weight: DWRITE_FONT_WEIGHT_BOLD,
                    designator: 1.0,
                    align: DWRITE_TEXT_ALIGNMENT_CENTER,
                };
                draw_text(context, &label, &font, badge, rgba(255, 255, 255, 1.0))?;
            }
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
            let brush =
                context.CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(0.92)), None)?;
            context.FillRoundedRectangle(&rounded(pill, px(3) / 2.0), &brush);
        }
    }
    Ok(())
}

fn color(Rgba(red, green, blue, alpha): Rgba) -> D2D1_COLOR_F {
    rgba(red, green, blue, alpha)
}

/// An outline as a `Canvas::body` shape: filled with `None`, stroked with a width.
fn geometry_shape<'a>(
    context: &'a ID2D1DeviceContext,
    outline: &'a ID2D1PathGeometry,
) -> impl Fn(&ID2D1Brush, Option<f32>) + 'a {
    move |brush, width| {
        // SAFETY: context is live between surface BeginDraw/EndDraw.
        unsafe {
            match width {
                None => context.FillGeometry(outline, brush, None),
                Some(width) => context.DrawGeometry(outline, brush, width, None),
            }
        }
    }
}

/// A gradient's stops, straight sRGB, clamped.
fn gradient(
    context: &ID2D1DeviceContext,
    stops: &[(f32, D2D1_COLOR_F)],
) -> Result<ID2D1GradientStopCollection1> {
    let stops: Vec<_> = stops
        .iter()
        .map(|&(position, color)| D2D1_GRADIENT_STOP { position, color })
        .collect();
    // SAFETY: context is live between surface BeginDraw/EndDraw; the stops are a local value the call copies.
    unsafe {
        context.CreateGradientStopCollection(
            &stops,
            D2D1_COLOR_SPACE_SRGB,
            D2D1_COLOR_SPACE_SRGB,
            D2D1_BUFFER_PRECISION_8BPC_UNORM,
            D2D1_EXTEND_MODE_CLAMP,
            D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT,
        )
    }
}

/// A linear gradient brush from `from` to `to` (in the brush's own space).
fn line(
    context: &ID2D1DeviceContext,
    from: (f32, f32),
    to: (f32, f32),
    stops: &[(f32, D2D1_COLOR_F)],
) -> Result<ID2D1LinearGradientBrush> {
    let stops = gradient(context, stops)?;
    // SAFETY: the properties are local values the call copies; the brush is scoped to the caller.
    unsafe {
        context.CreateLinearGradientBrush(
            &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: Vector2 {
                    X: from.0,
                    Y: from.1,
                },
                endPoint: Vector2 { X: to.0, Y: to.1 },
            },
            None,
            &stops,
        )
    }
}

/// Alpha of the glass themes' content backing (`glass_scrim`): a backdrop's text no longer shows through it (the
/// tab rail's fill is fully opaque).
const GLASS_BACKING: f32 = 0.97;

/// Glass themes only: a near-opaque body-coloured backing under the panel's content, following the body's own
/// outline inset by `layout::BACKING_INSET` so every label and control sits inside it and only an unoccupied
/// outer border stays translucent; it fades in with the panel (`reveal`). The pill, with no page, keeps the
/// plain glass.
fn glass_scrim(
    context: &ID2D1DeviceContext,
    body: &D2D_RECT_F,
    radius: f32,
    scale: f32,
    reveal: f32,
    p: &Palette,
) -> Result<()> {
    let inset = layout::BACKING_INSET * scale;
    // The top runs off the surface so only the lower corners round.
    let backing = D2D_RECT_F {
        left: body.left + inset,
        top: body.top - 24.0 * scale,
        right: body.right - inset,
        bottom: body.bottom - inset,
    };
    let Rgba(red, green, blue, _) = p.body;
    // SAFETY: a brush and a fill on the frame's own device context.
    unsafe {
        let brush = context
            .CreateSolidColorBrush(&color(Rgba(red, green, blue, GLASS_BACKING * reveal)), None)?;
        context.FillRoundedRectangle(&rounded(backing, (radius - inset).max(0.0)), &brush);
    }
    Ok(())
}

/// A 1 px glass rim: `ink` at the top alpha along `span.0` fading to the bottom alpha at `span.1` (device y).
fn rim_brush(
    context: &ID2D1DeviceContext,
    ink: (u8, u8, u8),
    (top, bottom): (f32, f32),
    span: (f32, f32),
) -> Result<ID2D1LinearGradientBrush> {
    line(
        context,
        (0.0, span.0),
        (0.0, span.1),
        &[(0.0, with_alpha(ink, top)), (1.0, with_alpha(ink, bottom))],
    )
}

/// Liquid glass, baked once: the body gradient, the specular blob and the lit rim, each a brush over the
/// unit square. `place` maps them onto a body with a brush transform, so a resize, a morphing notch or a
/// repaint never rebuilds them; the look is drawn, never sampled from behind the window.
#[derive(Clone)]
struct Glass {
    body: ID2D1LinearGradientBrush,
    spec: ID2D1RadialGradientBrush,
    rim: ID2D1LinearGradientBrush,
    /// A dark 1 px edge under the rim, so the glass keeps an outline over a bright page.
    edge: ID2D1SolidColorBrush,
}
impl Glass {
    fn place(&self, span: &D2D_RECT_F) {
        let map = Matrix3x2 {
            M11: span.right - span.left,
            M12: 0.0,
            M21: 0.0,
            M22: span.bottom - span.top,
            M31: span.left,
            M32: span.top,
        };
        // SAFETY: the brushes are owned by this canvas on the UI thread; the matrix is a local value.
        unsafe {
            self.body.SetTransform(&map);
            self.spec.SetTransform(&map);
            self.rim.SetTransform(&map);
        }
    }
}

/// Alpha of the dark edge under a glass rim.
const GLASS_EDGE: f32 = 0.30;

fn bake(context: &ID2D1DeviceContext, p: &Palette, sheen: &Sheen) -> Result<Glass> {
    let (lit, dim) = p.rim.unwrap_or_default();
    let white = |alpha| with_alpha((255, 255, 255), alpha);
    let blob = gradient(context, &[(0.0, white(sheen.spec)), (1.0, white(0.0))])?;
    Ok(Glass {
        body: line(
            context,
            (0.0, 0.0),
            (0.0, 1.0),
            &[
                (0.0, color(sheen.top)),
                (0.5, color(p.body)),
                (1.0, color(sheen.bottom)),
            ],
        )?,
        // Light from the upper left: bright along the top and left, dim along the bottom and right.
        rim: line(
            context,
            (0.0, 0.0),
            (0.4, 1.0),
            &[(0.0, with_alpha(p.ink, lit)), (1.0, with_alpha(p.ink, dim))],
        )?,
        // SAFETY: a brush on the frame's own device context.
        edge: unsafe { context.CreateSolidColorBrush(&rgba(0, 0, 0, GLASS_EDGE), None)? },
        // SAFETY: the properties are local values the call copies.
        spec: unsafe {
            context.CreateRadialGradientBrush(
                &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                    center: Vector2 { X: 0.3, Y: 0.0 },
                    gradientOriginOffset: Vector2 { X: 0.0, Y: 0.0 },
                    radiusX: 0.7,
                    radiusY: 0.9,
                },
                None,
                &blob,
            )?
        },
    })
}

/// The value built for `key`, kept in `slot`: built again only when the key changes (a theme change
/// gives the canvas a new slot anyway) and never on a plain repaint or resize, which the unit-square
/// glass does not depend on. A failed build leaves the slot as it was.
fn baked<K: PartialEq, T: Clone>(
    slot: &mut Option<(K, T)>,
    key: K,
    build: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if let Some((held, value)) = slot.as_ref()
        && *held == key
    {
        return Ok(value.clone());
    }
    let value = build()?;
    *slot = Some((key, value.clone()));
    Ok(value)
}

/// `bounds` (surface pixels) in the draw context's coordinates, where `origin` is the surface's rectangle.
fn surface_rect(origin: D2D_RECT_F, bounds: Bounds) -> D2D_RECT_F {
    D2D_RECT_F {
        left: origin.left + bounds.x as f32,
        top: origin.top + bounds.y as f32,
        right: origin.left + (bounds.x + bounds.width) as f32,
        bottom: origin.top + (bounds.y + bounds.height) as f32,
    }
}

/// A bar's track: a faint fill, the system track colour in high contrast, or a 1 px ink outline when
/// that colour would equal the dimmed fill's.
fn draw_track(
    context: &ID2D1DeviceContext,
    p: &Palette,
    track: &D2D1_ROUNDED_RECT,
    alpha: f32,
) -> Result<()> {
    // SAFETY: live draw context between BeginDraw/EndDraw; the brush is scoped to this call.
    unsafe {
        if p.track_outline() {
            let brush =
                context.CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(alpha)), None)?;
            context.DrawRoundedRectangle(track, &brush, 1.0, None);
        } else {
            let brush = context.CreateSolidColorBrush(&tint(p.track(alpha)), None)?;
            context.FillRoundedRectangle(track, &brush);
        }
    }
    Ok(())
}
fn tint(((red, green, blue), alpha): ((u8, u8, u8), f32)) -> D2D1_COLOR_F {
    rgba(red, green, blue, alpha)
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
        played(view, time, progress, dpi, None)
    }
    fn played<'a>(
        view: &'a View,
        time: &'a str,
        progress: f32,
        dpi: u32,
        played: Option<Progress>,
    ) -> MediaPlan<'a> {
        planned(view, time, progress, dpi, played, None)
    }
    fn volume<'a>(
        view: &'a View,
        time: &'a str,
        progress: f32,
        dpi: u32,
        volume: Option<Level>,
    ) -> MediaPlan<'a> {
        planned(view, time, progress, dpi, None, volume)
    }
    fn planned<'a>(
        view: &'a View,
        time: &'a str,
        progress: f32,
        dpi: u32,
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
            played,
            volume,
            bars: [0; BARS],
            timer: None,
            notice: None,
            shelf: &[],
            shelf_undo: false,
            widgets: crate::widgets::WidgetConfig::default(),
            tab: Tab::Media,
            rail: None,
            eyes: None,
            privacy: [0.0; 2],
            grip: Grip::default(),
            volume_fill: volume.map_or(0.0, |level| f32::from(level.percent)),
            weather: None,
            prayer: None,
            limits: None,
            leaving: None,
            fx: Fx::REST,
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
                played: None,
                volume: None,
                bars: [0; BARS],
                timer: None,
                notice: None,
                shelf: &[],
                shelf_undo: false,
                widgets: crate::widgets::WidgetConfig::default(),
                tab: Tab::Media,
                rail: None,
                eyes: None,
                privacy: [0.0; 2],
                grip: Grip::default(),
                volume_fill: 0.0,
                weather: None,
                prayer: None,
                limits: None,
                leaving: None,
                fx: Fx::REST,
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
        // The pill draws no controls.
        assert!(plan.chips.is_empty());
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
                    rect(145.0, 73.0, 163.0, 91.0),
                    Glyph::Previous,
                    false,
                    DISABLED
                ),
                (
                    rect(188.0, 66.0, 220.0, 98.0),
                    rect(196.0, 74.0, 212.0, 90.0),
                    Glyph::Pause,
                    true,
                    1.0
                ),
                (
                    rect(240.0, 66.0, 268.0, 98.0),
                    rect(245.0, 73.0, 263.0, 91.0),
                    Glyph::Next,
                    false,
                    1.0
                ),
                // Mute, disabled here: the scene has no audio endpoint.
                (
                    rect(140.0, 100.0, 164.0, 116.0),
                    rect(144.0, 100.0, 160.0, 116.0),
                    Glyph::Speaker,
                    false,
                    DISABLED
                ),
            ]
        );
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
        let panel = played(&song, "9:50 pm", 1.0, 96, Some(row));
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
            let panel = played(&song, "9:50 pm", 1.0, dpi, Some(row));
            let body = body(dpi, 1.0);
            let ([track, _], _) = panel.bar.unwrap();
            for item in panel.lines.iter().map(|l| l.rect).chain([track.rect]) {
                assert!(item.left >= body.left && item.right <= body.right, "{dpi}");
            }
        }
        // It fades in with the controls; collapsed or without a row, nothing is drawn.
        let opening = played(&song, "9:50 pm", 0.6, 96, Some(row));
        assert_eq!(opening.bar.unwrap().1, motion::reveal(0.6));
        assert!(played(&song, "9:50 pm", 0.0, 96, Some(row)).bar.is_none());
        let plain = plan(&song, "9:50 pm", 1.0, 96);
        assert!(plain.bar.is_none() && plain.lines.len() == 3);
    }

    #[test]
    fn controls_stay_inside_the_panel_body_at_any_scale() {
        let song = track("Song", "Artist", true);
        for dpi in [96, 144, 192] {
            let plan = plan(&song, "9:50 pm", 1.0, dpi);
            let body = body(dpi, 1.0);
            for item in plan.chips.iter().map(|c| c.rect) {
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
                played: None,
                volume: None,
                bars: [4, 20, 12, 8, 4],
                timer: None,
                notice: None,
                shelf: &[],
                shelf_undo: false,
                widgets: crate::widgets::WidgetConfig::default(),
                tab: Tab::Media,
                rail: None,
                eyes: None,
                privacy: [0.0; 2],
                grip: Grip::default(),
                volume_fill: 0.0,
                weather: None,
                prayer: None,
                limits: None,
                leaving: None,
                fx: Fx::REST,
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
    fn the_meter_update_rect_holds_every_bar_at_any_height_and_scale() {
        let song = track("Song", "Artist", true);
        for dpi in [96, 144, 192] {
            let window = layout::placement(
                Bounds {
                    x: 0,
                    y: 0,
                    width: 3840,
                    height: 2160,
                },
                dpi,
                SurfaceKind::Notch,
                0,
            )
            .width;
            let update = meter_update(window, dpi);
            let (slot, _) = layout::meter(dpi);
            for peak in [0.0, 0.5, 1.0] {
                let heights = layout::meter_heights([peak; BARS], dpi);
                let scene = NotchScene {
                    time: "9:50 pm",
                    date: "Tuesday",
                    track: song.shown(),
                    art: None,
                    progress: 1.0,
                    dpi,
                    played: None,
                    volume: None,
                    bars: heights,
                    timer: None,
                    notice: None,
                    shelf: &[],
                    shelf_undo: false,
                    widgets: crate::widgets::WidgetConfig::default(),
                    tab: Tab::Media,
                    rail: None,
                    eyes: None,
                    privacy: [0.0; 2],
                    grip: Grip::default(),
                    volume_fill: 0.0,
                    weather: None,
                    prayer: None,
                    limits: None,
                    leaving: None,
                    fx: Fx::REST,
                };
                let (bars, _) = meter_bars(&scene, &body(dpi, 1.0)).unwrap();
                for bar in bars {
                    let (left, top) = (update.left as f32, update.top as f32);
                    assert!(bar.rect.left >= left && bar.rect.right <= update.right as f32);
                    assert!(bar.rect.top >= top && bar.rect.bottom <= update.bottom as f32);
                }
            }
            assert_eq!(update.bottom - update.top, slot.height);
        }
    }

    #[test]
    fn the_speaker_follows_the_level_and_the_thumb_grows_with_the_pointer() {
        assert_eq!(Glyph::speaker(0, false), Glyph::Muted);
        assert_eq!(Glyph::speaker(70, true), Glyph::Muted);
        assert_eq!(Glyph::speaker(1, false), Glyph::SpeakerLow);
        assert_eq!(Glyph::speaker(33, false), Glyph::SpeakerLow);
        assert_eq!(Glyph::speaker(34, false), Glyph::Speaker);
        assert_eq!(Glyph::speaker(100, false), Glyph::Speaker);
        let song = track("Song", "Artist", true);
        let mut scene = NotchScene {
            time: "9:50 pm",
            date: "Tuesday",
            track: song.shown(),
            art: None,
            progress: 1.0,
            dpi: 96,
            played: None,
            volume: Some(Level {
                percent: 40,
                muted: false,
            }),
            bars: [0; BARS],
            timer: None,
            notice: None,
            shelf: &[],
            shelf_undo: false,
            widgets: crate::widgets::WidgetConfig::default(),
            tab: Tab::Media,
            rail: None,
            eyes: None,
            privacy: [0.0; 2],
            grip: Grip::default(),
            volume_fill: 0.0,
            weather: None,
            prayer: None,
            limits: None,
            leaving: None,
            fx: Fx::REST,
        };
        let radius = |scene: &NotchScene| volume_bar(scene, &body(96, 1.0)).unwrap().thumb.2;
        let rest = radius(&scene);
        scene.grip = Grip {
            hover: true,
            drag: false,
        };
        let hover = radius(&scene);
        scene.grip = Grip {
            hover: true,
            drag: true,
        };
        let press = radius(&scene);
        assert!(rest < hover && hover < press);
        let readout = volume_bar(&scene, &body(96, 1.0)).unwrap().readout;
        assert_eq!(readout.unwrap().0, "40%", "a press shows the percent");
    }

    #[test]
    fn volume_row_fills_to_the_shown_percent_and_dims_while_muted_or_unavailable() {
        let song = track("Song", "Artist", true);
        let level = |percent, muted| Some(Level { percent, muted });
        let panel = volume(&song, "9:50 pm", 1.0, 96, level(40, false));
        let bar = panel.volume.unwrap();
        // Bar x 172..346 in the body at 40; 6 DIP thick, centred in 100..116; the thumb sits on the fill end.
        assert_eq!(bar.track.rect, rect(172.0, 105.0, 346.0, 111.0));
        assert_eq!(bar.fill.rect, rect(172.0, 105.0, 172.0 + 70.0, 111.0));
        assert_eq!(bar.thumb, (242.0, 108.0, 4.5));
        assert!(bar.readout.is_none());
        assert_eq!((bar.track_alpha, bar.fill_alpha), (VOLUME_TRACK, 1.0));
        assert_eq!(
            (panel.chips[3].shape, panel.chips[3].alpha),
            (Glyph::Speaker, 1.0)
        );
        let muted = volume(&song, "9:50 pm", 1.0, 96, level(40, true));
        assert_eq!(muted.chips[3].shape, Glyph::Muted);
        assert_eq!(muted.chips[3].alpha, 1.0); // Still a live toggle.
        assert_eq!(muted.volume.unwrap().fill_alpha, MUTED_FILL);
        let unavailable = volume(&song, "9:50 pm", 1.0, 96, None);
        let bar = unavailable.volume.unwrap();
        assert_eq!((bar.fill.rect.right, bar.fill_alpha), (172.0, DISABLED));
        // The row is not drawn while collapsed.
        assert!(
            volume(&song, "9:50 pm", 0.0, 96, level(40, false))
                .volume
                .is_none()
        );
        let full = volume(&song, "9:50 pm", 1.0, 192, level(100, false))
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
    fn baked_glass_is_built_once_and_again_only_for_a_new_key_or_after_a_failure() {
        let (mut slot, mut builds) = (None, 0);
        let mut get = |key: u32, fail: bool| {
            baked(&mut slot, key, || {
                builds += 1;
                if fail {
                    Err(Error::new(E_FAIL, "build"))
                } else {
                    Ok(key * 10)
                }
            })
        };
        // Repaints and resizes ask with the same key: one build.
        assert_eq!(
            (
                get(1, false).unwrap(),
                get(1, false).unwrap(),
                get(1, true).unwrap()
            ),
            (10, 10, 10)
        );
        // A new key (another theme's sheen) rebuilds; a failed rebuild keeps the old value and retries.
        assert!(get(2, true).is_err());
        assert_eq!(get(2, false).unwrap(), 20);
        assert_eq!(get(2, false).unwrap(), 20);
        assert_eq!(builds, 3);
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

    #[test]
    fn the_hovered_icon_moves_smoothly_and_rests_on_whole_pixels() {
        use super::icon_metrics;
        // 125 %: icon 50 px, lift 6.25 px at full hover.
        assert_eq!(icon_metrics(0.0, 50.0, 120), (50.0, 0.0));
        assert_eq!(icon_metrics(1.0, 50.0, 120), (55.0, 6.0));
        let steps: Vec<(f32, f32)> = (1..100)
            .map(|i| icon_metrics(i as f32 / 100.0, 50.0, 120))
            .collect();
        // Mid-animation values are not rounded, and each hundredth of the travel moves them a little.
        assert!(
            steps
                .iter()
                .any(|&(size, lift)| size.fract() != 0.0 && lift.fract() != 0.0)
        );
        assert!(steps.windows(2).all(|w| w[1].0 > w[0].0 && w[1].1 > w[0].1));
        // The overshoot goes past rest and the next frame comes back to it.
        assert!(icon_metrics(1.02, 50.0, 120).1 > 6.25);
    }
}
