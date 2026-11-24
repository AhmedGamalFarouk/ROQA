mod dock_art;
use crate::{
    audio::{BARS, Level},
    dock,
    edge::{ListRow, MIX_PERCENT},
    eyes::{Frame, Mood, Pose},
    icons::IconPixels,
    layout::{self, Bounds},
    media::{PLAY_PAUSE, Progress, Track},
    motion,
    power::Extra,
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
            D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP,
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
mod moon;
pub use glyphs::{draw_gdi as draw_settings_glyph, round_gdi};
mod ask;
mod limit_row;
mod media;
mod notice_footer;
mod shot;
mod tab_art;
use ask::wrapped;
pub use ask::{ask_fits, ask_fits_at};
pub use glyphs::Glyph;
pub use media::COVER_DIPS;
use media::{media_page, meter_rects, meter_update};
pub use shot::ShotScene;

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

/// What one dock slot shows: a real app group.
pub struct DockSlot<'a> {
    pub icon: Option<&'a IconPixels>,
    pub mini: Vec<Option<&'a IconPixels>>,
    pub running: bool,
    /// A recently closed app: its icon is drawn at half strength.
    pub faded: bool,
    /// Open windows in the group: a resting group shows up to three dots, one per window.
    pub windows: usize,
    pub active: bool,
    pub badge: dock::Badge,
    /// Hover animation progress, 0 (rest) to 1 (hovered).
    pub hover: f32,
    /// Pixels the slot sits from its cell while a drag opens a gap, or follows the pointer when it is the held one.
    pub dx: f32,
    /// The icon being dragged: drawn last, over the others.
    pub held: bool,
}

/// The dock widgets' readings, and whether the Timer chip's countdown is paused (drawn at 0.60 ink).
#[derive(Default, PartialEq)]
pub struct Readings {
    pub text: [String; 6],
    pub timer_paused: bool,
}

pub struct DockScene<'a> {
    /// Window x of the drop marker (reduced motion draws a Lamp capsule there instead of sliding the icons).
    pub marker: Option<f32>,
    pub slots: Vec<DockSlot<'a>>,
    /// Which taskbar buttons are on, and each enabled one's hover progress.
    pub layout: dock::Buttons,
    pub buttons: Vec<(dock::Button, f32)>,
    pub widget_readings: &'a Readings,
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
    /// The Umm al-Qura date under the date line; empty draws nothing.
    pub hijri: &'a str,
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
    /// The idle Timer page's waiting reminders as (time, words), at most `layout::REMIND_ROWS`.
    pub reminders: &'a [(String, String)],
    /// The power pill's text (features B1), drawn in the open panel; `None` draws nothing.
    pub notice: Option<&'a str>,
    /// What the notice row carries besides its text: an Eject button or a progress bar.
    pub extra: Extra,
    pub footer: Option<layout::NoticeFooter>,
    /// The screenshot pill on the notice row while it shows (no notice then).
    pub shot: Option<ShotScene<'a>>,
    /// The file shelf's visible entries (S6, PROVISIONAL); empty draws nothing.
    pub shelf: &'a [ShelfSlot<'a>],
    pub shelf_undo: bool,
    pub shelf_hover: bool,
    /// The Shelf shows its Copied list: `shelf` then holds copied text (an empty list is the empty state).
    pub clips: bool,
    /// The page the open panel shows. Media is the media and clock panel; the others draw their own content.
    pub tab: Tab,
    pub low_battery: bool,
    pub rail: Option<RailScene<'a>>,
    /// The mascot's frame (`None` with the setting off); alpha 0 at rest, when the pill shows only the time.
    pub eyes: Option<Frame>,
    /// Opacity of the microphone (orange), camera (green) and muted-mic (accent) dots at the right of the pill and header.
    pub privacy: [f32; 3],
    /// The volume bar's pointer state: the thumb grows on hover and press, and a press shows the percent.
    pub grip: Grip,
    /// The wash behind the Media page's glance line, 0 to 1 (hover; drawn only behind a glance).
    pub line_wash: f32,
    /// The volume bar's drawn fill in percent: it eases toward `volume`'s percent.
    pub volume_fill: f32,
    /// The Weather page's text and pictures; `Some` only while that page shows.
    pub weather: Option<&'a crate::weather::View>,
    /// The Prayer page's text; `Some` only while that page shows.
    pub prayer: Option<&'a crate::prayer_tab::View>,
    /// The page a switch is leaving, drawn fading out beside the new one while `fx.page` runs.
    pub leaving: Option<Tab>,
    pub fx: Fx,
}

/// The tab handle: its geometry, the cell under the pointer and the shown tabs in order.
pub struct RailScene<'a> {
    pub layout: layout::Rail,
    pub tabs: &'a [Tab],
    /// The sliding indicator's fractional tab position, its arrival pop (0 to 1, overshoots), the fading
    /// hover wash (tab, strength) and the press squash (0 to 1).
    pub ind: f32,
    pub pop: f32,
    pub press: f32,
}

/// One shelf entry and whether
/// the last activation found the target gone.
pub struct ShelfSlot<'a> {
    /// The file name shown under the icon; empty for the add and more slots.
    pub name: &'a str,
    pub missing: bool,
    pub more: bool,
    pub add: bool,
    /// A kept screenshot's thumbnail, drawn in place of the file glyph.
    pub thumb: Option<&'a IconPixels>,
}

/// Dock material (docs/design/roqa-visual-spec.md, colours from the theme): bar, ink-alpha hover fill, whole-pixel icon lift.
const ICON_LIFT_DIPS: f32 = 5.0;
const TEXT_FONT: PCWSTR = w!("Segoe UI Variable Text");
const DISPLAY_FONT: PCWSTR = w!("Segoe UI Variable Display");
/// The time's digits lead; "pm" and similar designators sit smaller beside them.
const TIME_DESIGNATOR: f32 = 0.72;
/// Opacity of a control the player does not accept now.
const DISABLED: f32 = 0.30;

/// Dock labels share a 190 DIP text cap and 48 DIP budget for row insets and pixel rounding.
pub fn dock_label_width(label: &str, dpi: u32) -> Result<i32> {
    let scale = dpi as f32 / 96.0;
    let font = Font {
        family: TEXT_FONT,
        size: 13.0 * scale,
        weight: DWRITE_FONT_WEIGHT_NORMAL,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_LEADING,
    };
    Ok((text_width(label, &font)?.min(190.0 * scale) + 48.0 * scale).ceil() as i32)
}

pub struct ListScene<'a> {
    pub reveal: f32,
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
    /// The screenshot chip's two thumbnails, kept like `art`.
    shots: shot::Thumbs,
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
                shots: RefCell::default(),
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
            let open = notice_footer::shadow_strength(scene);
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
                (p.ink.0 < 128).then_some(0.14),
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
                // The backing would flatten every glass to one tint: put the light back on top of it, Liquid's
                // baked sheen and lit rim, Frosted's plain rim.
                let shape = geometry_shape(context, &outline);
                match self.glass(context)? {
                    Some(glass) => {
                        shape(&glass.spec, None);
                        shape(&glass.rim, Some(1.0));
                    }
                    None => {
                        let rim = p.rim.unwrap_or_default();
                        let brush = rim_brush(context, p.ink, rim, (body.top, body.bottom))?;
                        shape(&brush, Some(1.0));
                    }
                }
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
                .and_then(|art| notch_text(context, scene, &body, scale, art.as_ref(), status, &p))
                .and_then(|()| shot::chip(context, scene, &body, scale, &p, &self.shots));
            // SAFETY: balances the PushAxisAlignedClip above on the same live context.
            unsafe { context.PopAxisAlignedClip() };
            drawn?;
            notice_footer::draw(context, scene, &body, &outline, &p)?;
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
    /// The volume slider's device list: a panel of rows, the current device marked with an accent dot
    /// and the hovered row lifted.
    pub fn paint_list(&self, scene: &ListScene) -> Result<()> {
        let p = self.palette;
        let scale = scene.dpi as f32 / 96.0;
        self.draw(|context, rect| {
            let panel = surface_rect(rect, scene.panel);
            let progress = scene.reveal;
            let stretch = 0.94 + 0.06 * progress;
            let center = (panel.left + panel.right) / 2.0;
            // SAFETY: the live draw context copies this transform before drawing the popup.
            unsafe {
                context.SetTransform(&Matrix3x2 {
                    M11: stretch,
                    M12: 0.0,
                    M21: 0.0,
                    M22: stretch,
                    M31: center * (1.0 - stretch),
                    M32: panel.bottom * (1.0 - stretch),
                });
            }
            // Every popover shares one radius: the row capsule's (17) plus the list inset (6).
            self.panel(
                context,
                panel,
                (scene.panel.height as f32 / 2.0).min(23.0 * scale),
            )?;
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
                if let Some((lit, right)) = row.moon {
                    moon::hero(context, cell, lit, right, row.name, scale, &p)?;
                    continue;
                }
                if let Some((action, on, details)) = row.quick {
                    dock_art::quick_tile(
                        context,
                        cell,
                        row.name,
                        (action, on, details),
                        scene.hover == Some(index),
                        scale,
                        &p,
                    )?;
                    continue;
                }
                if let Some(today) = row.calendar {
                    dock_art::calendar_week(context, cell, row.name, today, &font, &p)?;
                    continue;
                }
                if let Some(value) = row.value {
                    moon::date_row(context, cell, row.name, value, scale, &p)?;
                    continue;
                }
                let (left, middle) = (cell.left + 12.0 * scale, (cell.top + cell.bottom) / 2.0);
                // A mixer row is a capsule slider: the track, and the level filled in the accent.
                if let Some((percent, muted)) = row.level {
                    let shape = rounded(cell, (cell.bottom - cell.top) / 2.0);
                    draw_track(context, &p, &shape, 0.30)?;
                    let width = (cell.right - cell.left) * f32::from(percent.min(100)) / 100.0;
                    if width >= 1.0 {
                        let fill = D2D_RECT_F {
                            right: cell.left + width,
                            ..cell
                        };
                        // SAFETY: live draw context between BeginDraw/EndDraw; the brush is scoped to this draw.
                        unsafe {
                            let brush = context
                                .CreateSolidColorBrush(&tint(p.accent_dim(muted, 0.40)), None)?;
                            context.FillRoundedRectangle(
                                &rounded(fill, ((cell.bottom - cell.top) / 2.0).min(width / 2.0)),
                                &brush,
                            );
                        }
                    }
                }
                // SAFETY: live draw context between BeginDraw/EndDraw; brushes are scoped to this draw.
                unsafe {
                    if scene.hover == Some(index) {
                        let brush = context
                            .CreateSolidColorBrush(&tint(p.accent_dim(false, 0.22)), None)?;
                        context.FillRoundedRectangle(
                            &rounded(cell, (cell.bottom - cell.top) / 2.0),
                            &brush,
                        );
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
                if let Some(percent) = row.bar {
                    limit_row::paint(context, &p, cell, row.name, percent, scale)?;
                    continue;
                }
                // Only a marked row keeps room for its dot; the others start on the inset.
                let mut text = D2D_RECT_F {
                    left: if row.current {
                        left + dot + 8.0 * scale
                    } else {
                        left
                    },
                    right: cell.right - 8.0 * scale,
                    ..cell
                };
                // A mixer or stack row keeps room for its icon while the icon is still on its way.
                if row.icon.is_some() || row.level.is_some() || row.room {
                    let side = 20.0 * scale;
                    if let Some(pixels) = row.icon {
                        let frame = D2D_RECT_F {
                            left,
                            top: middle - side / 2.0,
                            right: left + side,
                            bottom: middle + side / 2.0,
                        };
                        let muted = row.level.is_some_and(|(_, muted)| muted);
                        // SAFETY: live draw context between BeginDraw/EndDraw; the bitmap is scoped to this draw.
                        unsafe {
                            context.DrawBitmap(
                                &upload(context, pixels)?,
                                Some(&frame),
                                if muted { 0.40 } else { 1.0 },
                                D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                                None,
                                None,
                            );
                        }
                    }
                    text.left = left + side + 8.0 * scale;
                }
                if let Some((percent, muted)) = row.level {
                    text.right = cell.right - MIX_PERCENT as f32 * scale;
                    let label = if muted {
                        "Muted".to_owned()
                    } else {
                        format!("{percent}%")
                    };
                    let digits = D2D_RECT_F {
                        left: text.right,
                        right: cell.right - 14.0 * scale,
                        ..cell
                    };
                    let trailing = Font {
                        align: DWRITE_TEXT_ALIGNMENT_TRAILING,
                        ..font
                    };
                    draw_text(
                        context,
                        &label,
                        &trailing,
                        digits,
                        tint(p.ink_dim(false, 0.90)),
                    )?;
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
                    None,
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
        self.body(context, &rect, flat, None, &|brush, width| {
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
    let glance = notice_footer::glance_body(scene, *body, scale);
    if let Some(frame) = scene.eyes {
        tab_art::draw_eyes(context, scene, frame, &glance, scale, p)?;
    }
    tab_art::draw_privacy(context, scene, &glance, scale, p)?;
    tab_art::draw_low(context, scene, &glance, scale, p)?;
    if scene.tab == Tab::Media {
        media_page(context, scene, &glance, scale, art, status, p)?;
    } else {
        // Another page: the pill's content fades out as the page fades in.
        let reveal = motion::reveal(scene.progress);
        tab_art::faded(context, 1.0 - reveal, || {
            media_page(context, scene, &glance, scale, art, status, p)
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
        context.FillRoundedRectangle(
            &rounded(background, (background.bottom - background.top) / 2.0),
            &brush,
        );
    }
    glyphs::draw(
        context,
        square,
        Glyph::Gear,
        with_alpha(p.ink, p.ink_alpha(0.9 * reveal)),
    )
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
    // The breathing ring at the pill's left, in the accent: a function of the wall clock, repainted by the host
    // only while the pill is on screen.
    if !view.done && !view.paused {
        let centre = Vector2 {
            X: body.left + 21.0 * scale,
            Y: body.top + 15.0 * scale,
        };
        // SAFETY: a brush and an outline on the frame's own device context.
        unsafe {
            let brush = context.CreateSolidColorBrush(
                &with_alpha(p.accent, p.ink_alpha(motion::breath() * (1.0 - reveal))),
                None,
            )?;
            context.DrawEllipse(
                &D2D1_ELLIPSE {
                    point: centre,
                    radiusX: 4.5 * scale,
                    radiusY: 4.5 * scale,
                },
                &brush,
                2.0 * scale,
                None,
            );
        }
    }
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
    if scene.extra.footer() {
        return Ok(());
    }
    let reveal = motion::reveal(scene.progress);
    let Some(text) = scene.notice.filter(|_| reveal > 0.0) else {
        return Ok(());
    };
    let slot = layout::power_notice(scene.track.is_some(), scene.dpi);
    // Buttons from the right: the first is the alert's one primary action, a solid accent capsule; a second is quiet.
    let labels: &[&str] = match scene.extra {
        Extra::Eject(_) => &["Eject"],
        // Deny is the primary (accent) button, rightmost; Allow is quiet.
        Extra::Ask(_) => &["Deny", "Allow"],
        _ => &[],
    };
    let button = (!labels.is_empty())
        .then(|| layout::notice_button(scene.track.is_some(), scene.dpi, labels.len() - 1));
    // A progress row gives the text its left share and the bar the rest.
    let bar_left = matches!(scene.extra, Extra::Progress(_)).then(|| slot.width as f32 * 0.5);
    let left = body.left + slot.x as f32;
    // The notice rises 5 DIP into place as it fades in.
    let top = body.top + slot.y as f32 + (1.0 - scene.fx.notice) * 5.0 * scale;
    // A permission question is two lines of the command, a little smaller.
    let ask = layout::notice_ask();
    let font = Font {
        family: TEXT_FONT,
        size: if ask { 11.0 } else { 12.0 } * scale,
        weight: DWRITE_FONT_WEIGHT_NORMAL,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_CENTER,
    };
    // The text gives the button its place at the row's right.
    let right = bar_left.unwrap_or_else(|| {
        button.map_or(slot.width as f32, |button| {
            (button.x - slot.x) as f32 - 6.0 * scale
        })
    });
    let rect = D2D_RECT_F {
        left,
        top,
        right: left + right,
        bottom: top + slot.height as f32,
    };
    let alpha = reveal * scene.fx.notice;
    let colour = with_alpha(p.ink, p.ink_alpha(0.85 * alpha));
    let width = rect.right - rect.left;
    let lines = if ask {
        wrapped(text, width, |line| {
            text_width(line, &font).is_ok_and(|w| w <= width)
        })
    } else {
        [text.to_owned(), String::new()]
    };
    if lines[1].is_empty() {
        draw_text(context, &lines[0], &font, rect, colour)?;
    } else {
        let half = (rect.bottom - rect.top) / 2.0;
        for (index, line) in lines.iter().enumerate() {
            let top = rect.top + half * index as f32;
            let row = D2D_RECT_F {
                top,
                bottom: top + half,
                ..rect
            };
            draw_text(context, line, &font, row, colour)?;
        }
    }
    if let (Some(left_edge), Extra::Progress(percent)) = (bar_left, scene.extra) {
        // A quiet track with the accent fill: the one progress fill.
        let height = 4.0 * scale;
        let mid = top + slot.height as f32 / 2.0;
        let track = D2D_RECT_F {
            left: left + left_edge + 6.0 * scale,
            top: mid - height / 2.0,
            right: left + slot.width as f32,
            bottom: mid + height / 2.0,
        };
        let fill = D2D_RECT_F {
            right: track.left + (track.right - track.left) * f32::from(percent) / 100.0,
            ..track
        };
        // SAFETY: context is live between surface BeginDraw/EndDraw; the brushes are scoped to this draw.
        unsafe {
            let back = context
                .CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(0.18 * alpha)), None)?;
            context.FillRoundedRectangle(&rounded(track, height / 2.0), &back);
            let front =
                context.CreateSolidColorBrush(&with_alpha(p.accent, p.ink_alpha(alpha)), None)?;
            context.FillRoundedRectangle(&rounded(fill, height / 2.0), &front);
        }
    }
    for (from_right, name) in labels.iter().enumerate() {
        let button = layout::notice_button(scene.track.is_some(), scene.dpi, from_right);
        let cell_top = top + (button.y - slot.y) as f32;
        let cell = D2D_RECT_F {
            left: body.left + button.x as f32,
            top: cell_top,
            right: body.left + (button.x + button.width) as f32,
            bottom: cell_top + button.height as f32,
        };
        let primary = from_right == 0;
        let (fill, ink) = if primary {
            (with_alpha(p.accent, p.ink_alpha(alpha)), p.ink_on_accent())
        } else {
            (with_alpha(p.ink, p.ink_alpha(0.18 * alpha)), p.ink)
        };
        // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw.
        unsafe {
            let fill = context.CreateSolidColorBrush(&fill, None)?;
            context.FillRoundedRectangle(&rounded(cell, (cell.bottom - cell.top) / 2.0), &fill);
        }
        let label = Font {
            size: 11.0 * scale,
            ..font
        };
        draw_text(
            context,
            name,
            &label,
            cell,
            with_alpha(ink, p.ink_alpha(alpha)),
        )?;
    }
    Ok(())
}

/// What the pointer is doing on the volume bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grip {
    pub hover: bool,
    pub drag: bool,
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
    draw_layout(context, &layout, rect, color)
}

fn draw_layout(
    context: &ID2D1DeviceContext,
    layout: &IDWriteTextLayout,
    rect: D2D_RECT_F,
    color: D2D1_COLOR_F,
) -> Result<()> {
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw.
    unsafe {
        let brush = context.CreateSolidColorBrush(&color, None)?;
        context.DrawTextLayout(
            Vector2 {
                X: rect.left,
                Y: rect.top,
            },
            layout,
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
    // The held icon last, so it rides over its neighbours.
    for held in [false, true] {
        for (index, slot) in scene.slots.iter().take(visible).enumerate() {
            if slot.held == held {
                let mut at = cell(dock::Hit::Slot(index));
                (at.left, at.right) = (at.left + slot.dx, at.right + slot.dx);
                draw_slot(context, &at, slot, scene.dpi, px, p)?;
            }
        }
    }
    for &(button, hover) in &scene.buttons {
        draw_button(
            context,
            &cell(dock::Hit::Button(button)),
            button,
            hover,
            scene,
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
                left: at - px(3) / 2.0,
                top: rect.top + px(20),
                right: at + px(3) / 2.0,
                bottom: rect.bottom - px(20),
            };
            context.FillRoundedRectangle(&rounded(hairline, px(3) / 2.0), &line);
        }
        if let Some(x) = scene.marker {
            let mark = D2D_RECT_F {
                left: rect.left + x - px(3) / 2.0,
                top: (rect.top + rect.bottom) / 2.0 - px(18),
                right: rect.left + x + px(3) / 2.0,
                bottom: (rect.top + rect.bottom) / 2.0 + px(18),
            };
            let lamp = context.CreateSolidColorBrush(&tint(p.accent_dim(false, 1.0)), None)?;
            context.FillRoundedRectangle(&rounded(mark, px(3) / 2.0), &lamp);
        }
    }
    Ok(())
}

/// The one box (DIPs) every dock button glyph is stroked in: about 19 DIPs of ink beside the 40 DIP icons.
const DOCK_GLYPH: i32 = 26;

/// One taskbar button: the hover chip, then its glyph.
fn draw_button(
    context: &ID2D1DeviceContext,
    cell: &D2D_RECT_F,
    button: dock::Button,
    hover: f32,
    scene: &DockScene,
    px: &dyn Fn(i32) -> f32,
    p: &Palette,
) -> Result<()> {
    let layout = scene.layout;
    let reading = button
        .widget()
        .map(|index| scene.widget_readings.text[index].as_str());
    let ink = if button == dock::Button::Timer && scene.widget_readings.timer_paused {
        0.60
    } else {
        1.0
    };
    if button == dock::Button::Desktop {
        return desktop_pill(context, cell, hover, layout.rtl, px, p);
    }
    let centre = (
        (cell.left + cell.right) / 2.0,
        (cell.top + cell.bottom) / 2.0,
    );
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
            let brush =
                context.CreateSolidColorBrush(&tint(p.accent_dim(false, 0.18 * hover)), None)?;
            context.FillRoundedRectangle(&rounded(chip, half_h), &brush);
        }
    }
    if button == dock::Button::Start {
        return windows_logo(context, centre, px);
    }
    let has_reading = reading.is_some_and(|text| !text.is_empty());
    let side = px(if reading.is_some() { 20 } else { DOCK_GLYPH });
    let left = if has_reading {
        cell.left + px(9)
    } else {
        centre.0 - side / 2.0
    };
    let square = pixel_square(left, centre.1 - side / 2.0, side);
    if let Some(text) = reading.filter(|_| has_reading) {
        draw_text(
            context,
            text,
            &Font {
                family: windows::core::w!("Segoe UI Variable"),
                size: px(13),
                weight: DWRITE_FONT_WEIGHT_NORMAL,
                designator: 1.0,
                align: DWRITE_TEXT_ALIGNMENT_LEADING,
            },
            D2D_RECT_F {
                left: square.right + px(5),
                right: cell.right - px(5),
                ..*cell
            },
            tint(p.ink_dim(false, ink)),
        )?;
    }
    if let (dock::Button::Moon, Some((pct, right))) = (button, layout.moon) {
        let crate::theme::Rgba(r, g, b, _) = p.body;
        let ink = glyphs::moon_icon(p.glyph, with_alpha((r, g, b), 1.0), side);
        return glyphs::moon(context, square, f32::from(pct) / 100.0, right, &ink);
    }
    let shape = match button {
        dock::Button::Start => unreachable!("drawn above"),
        dock::Button::Search => Glyph::Search,
        dock::Button::TaskView => Glyph::TaskView,
        dock::Button::Desktop => unreachable!("drawn above"),
        dock::Button::Quick => Glyph::Sliders,
        dock::Button::Weather => Glyph::PartlyCloudy,
        dock::Button::Prayer => Glyph::Prayer,
        dock::Button::Calendar => Glyph::Widgets,
        dock::Button::Water => Glyph::Drizzle,
        dock::Button::System => Glyph::Drive,
        dock::Button::Timer => Glyph::Timer,
        dock::Button::Places => Glyph::Drive,
        dock::Button::Tray => Glyph::Chevron,
        dock::Button::Battery => Glyph::Battery,
        dock::Button::Limits => Glyph::Limits,
        dock::Button::Volume => Glyph::Speaker,
        dock::Button::Brightness => Glyph::Sun,
        dock::Button::Moon => return Ok(()), // drawn above while switched on
    };
    glyphs::draw(context, square, shape, with_alpha(p.glyph, ink))
}

/// The show-desktop capsule brightens with `hover`; the whole cell is the click target.
fn desktop_pill(
    context: &ID2D1DeviceContext,
    cell: &D2D_RECT_F,
    hover: f32,
    rtl: bool,
    px: &dyn Fn(i32) -> f32,
    p: &Palette,
) -> Result<()> {
    let half_w = px(3) + px(1) * hover;
    let centre_x = if rtl {
        cell.left + px(14)
    } else {
        cell.right - px(14)
    };
    let mid = (cell.top + cell.bottom) / 2.0;
    let pill = D2D_RECT_F {
        left: centre_x - half_w,
        top: mid - px(14),
        right: centre_x + half_w,
        bottom: mid + px(14),
    };
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw.
    unsafe {
        let brush =
            context.CreateSolidColorBrush(&with_alpha(p.glyph, 0.38 + 0.52 * hover), None)?;
        context.FillRoundedRectangle(&rounded(pill, half_w), &brush);
    }
    Ok(())
}

/// The Windows 11 logo for the Start button: four equal squares, a gap of a ninth of the side, in Windows blue.
/// It is a brand mark like an app icon, so it ignores theme and accent. Whole device pixels keep the edges crisp.
fn windows_logo(
    context: &ID2D1DeviceContext,
    centre: (f32, f32),
    px: &dyn Fn(i32) -> f32,
) -> Result<()> {
    let side = px(18).round();
    let gap = (side / 9.0).round().max(1.0);
    let square = ((side - gap) / 2.0).floor();
    let origin = (
        (centre.0 - (2.0 * square + gap) / 2.0).round(),
        (centre.1 - (2.0 * square + gap) / 2.0).round(),
    );
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw.
    unsafe {
        let brush = context.CreateSolidColorBrush(&rgba(0x00, 0x78, 0xD4, 1.0), None)?;
        for (column, row) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let left = origin.0 + column * (square + gap);
            let top = origin.1 + row * (square + gap);
            context.FillRectangle(
                &D2D_RECT_F {
                    left,
                    top,
                    right: left + square,
                    bottom: top + square,
                },
                &brush,
            );
        }
    }
    Ok(())
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
            let brush = context
                .CreateSolidColorBrush(&tint(p.accent_dim(false, 0.18 * slot.hover)), None)?;
            context.FillRoundedRectangle(&rounded(highlight, px(24)), &brush);
        }
        if !slot.mini.is_empty() {
            let brush = context.CreateSolidColorBrush(&tint(p.track(0.16)), None)?;
            // A squircle (radius = tile radius less the inset) holding a 2x2 grid; empty cells stay as quiet wells.
            context.FillRoundedRectangle(&rounded(frame, size * 0.3), &brush);
            let well = context.CreateSolidColorBrush(&tint(p.track(0.10)), None)?;
            for index in 0..4 {
                let (column, row, fraction) = dock::mini_cell(index).unwrap();
                let side = size * fraction;
                let left = frame.left + size * column;
                let top = frame.top + size * row;
                let mini = D2D_RECT_F {
                    left,
                    top,
                    right: left + side,
                    bottom: top + side,
                };
                let Some(icon) = slot.mini.get(index) else {
                    context.FillRoundedRectangle(&rounded(mini, size * 0.1), &well);
                    continue;
                };
                if let Some(pixels) = icon {
                    context.DrawBitmap(
                        &upload(context, pixels)?,
                        Some(&mini),
                        1.0,
                        D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                        None,
                        None,
                    );
                } else {
                    glyphs::draw(context, mini, Glyph::Apps, with_alpha(p.glyph, 1.0))?;
                }
            }
        } else if let Some(pixels) = slot.icon {
            // shortcut: bitmaps are rebuilt per paint (catalog/icon/hover changes and the frames of a ~0.2 s hover animation); cache per device if profiling shows cost.
            context.DrawBitmap(
                &upload(context, pixels)?,
                Some(&frame),
                if slot.faded { 0.5 } else { 1.0 },
                D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                None,
                None,
            );
        } else {
            // A real group whose icon is unavailable or still loading: neutral tile, never a fake app icon.
            let brush = context.CreateSolidColorBrush(&color(p.tile), None)?;
            // A capsule disc, the one tile shape.
            context.FillRoundedRectangle(&rounded(frame, size / 2.0), &brush);
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
            let brush = context.CreateSolidColorBrush(&tint(p.track(1.0)), None)?;
            context.FillRoundedRectangle(&rounded(badge, height / 2.0), &brush);
            if let Some(label) = label {
                let font = Font {
                    family: TEXT_FONT,
                    size: px(10),
                    weight: DWRITE_FONT_WEIGHT_BOLD,
                    designator: 1.0,
                    align: DWRITE_TEXT_ALIGNMENT_CENTER,
                };
                draw_text(context, &label, &font, badge, with_alpha(p.on_ink, 1.0))?;
            }
        }
        if slot.active || slot.running {
            // Focus is a full Lamp capsule (the selected indicator); other running groups a softer ink dot.
            let (half, bottom) = (px(if slot.active { 6 } else { 2 }), cell.bottom - px(4));
            let colour = if slot.active {
                p.accent_dim(false, 1.0)
            } else {
                p.ink_dim(false, 0.7)
            };
            let brush = context.CreateSolidColorBrush(&tint(colour), None)?;
            let dots = if slot.active {
                1
            } else {
                slot.windows.clamp(1, 3)
            };
            for dot in 0..dots {
                let x = center_x + (dot as f32 - (dots - 1) as f32 / 2.0) * px(6);
                let pill = D2D_RECT_F {
                    left: x - half,
                    top: bottom - px(3),
                    right: x + half,
                    bottom,
                };
                context.FillRoundedRectangle(&rounded(pill, px(3) / 2.0), &brush);
            }
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
