//! Every drawn icon (features I1, direction B "crisp line"): one 24-unit grid, 1.75-unit strokes with round
//! caps and joins, no fills, one `DrawGeometry` per icon. Each glyph is one SVG-style path string; the cached
//! geometry is built once per Direct2D factory, and the stroke is snapped to whole device pixels so lines
//! stay sharp at any DPI. No raster, no icon font.

use super::*;
use crate::weather::Icon;
use std::cell::RefCell;
use windows::Win32::Graphics::Direct2D::{
    Common::{D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN},
    D2D1_ARC_SIZE_LARGE, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID, D2D1_LINE_JOIN_ROUND,
    D2D1_STROKE_STYLE_PROPERTIES, ID2D1Factory, ID2D1StrokeStyle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Play,
    Pause,
    Previous,
    Next,
    /// The mute toggle: a speaker with two sound waves, one wave at a low level, none while muted.
    Speaker,
    SpeakerLow,
    Muted,
    /// The settings button: a cog.
    Gear,
    /// The brightness slider's icon: a half-filled sun.
    Sun,
    Clear,
    PartlyCloudy,
    Cloud,
    Fog,
    Drizzle,
    Rain,
    Snow,
    Storm,
    Prayer,
    Timer,
    Shelf,
    Limits,
    Start,
    /// The dock picker: a grid of nine dots, not the Start panes.
    Apps,
    Search,
    TaskView,
    Chevron,
    Expand,
    Desktop,
    Charging,
    Add,
    More,
    /// A shelf entry whose system icon did not load.
    File,
}
const ALL: [Glyph; 32] = [
    Glyph::Play,
    Glyph::Pause,
    Glyph::Previous,
    Glyph::Next,
    Glyph::Speaker,
    Glyph::SpeakerLow,
    Glyph::Muted,
    Glyph::Gear,
    Glyph::Sun,
    Glyph::Clear,
    Glyph::PartlyCloudy,
    Glyph::Cloud,
    Glyph::Fog,
    Glyph::Drizzle,
    Glyph::Rain,
    Glyph::Snow,
    Glyph::Storm,
    Glyph::Prayer,
    Glyph::Timer,
    Glyph::Shelf,
    Glyph::Limits,
    Glyph::Start,
    Glyph::Apps,
    Glyph::Search,
    Glyph::TaskView,
    Glyph::Chevron,
    Glyph::Expand,
    Glyph::Desktop,
    Glyph::Charging,
    Glyph::Add,
    Glyph::More,
    Glyph::File,
];
/// Grid size and stroke, in grid units.
const GRID: f32 = 24.0;
const STROKE: f32 = 1.75;

pub(super) fn active_ink(palette: &Palette) -> (u8, u8, u8) {
    if palette.high_contrast.is_some() {
        palette.on_ink
    } else {
        palette.accent
    }
}

impl Glyph {
    /// The speaker for a level: silent or muted shows no waves, up to a third one wave, above it two.
    pub fn speaker(percent: u8, muted: bool) -> Glyph {
        match percent {
            _ if muted => Glyph::Muted,
            0 => Glyph::Muted,
            1..=33 => Glyph::SpeakerLow,
            _ => Glyph::Speaker,
        }
    }
    pub(super) fn weather(icon: Icon) -> Glyph {
        match icon {
            Icon::Clear => Glyph::Clear,
            Icon::PartlyCloudy => Glyph::PartlyCloudy,
            Icon::Cloud => Glyph::Cloud,
            Icon::Fog => Glyph::Fog,
            Icon::Drizzle => Glyph::Drizzle,
            Icon::Rain => Glyph::Rain,
            Icon::Snow => Glyph::Snow,
            Icon::Storm => Glyph::Storm,
        }
    }
    /// The glyph's path on the 24-unit grid (SVG syntax: M L H V A Z, absolute or relative).
    fn path(self) -> &'static str {
        match self {
            Glyph::Play => {
                "M8.5 5.8v12.4a.6.6 0 0 0 .9.5l9.6-6.2a.6.6 0 0 0 0-1L9.4 5.3a.6.6 0 0 0-.9.5z"
            }
            Glyph::Pause => {
                "M8 5.5h1.8a.7.7 0 0 1 .7.7v11.6a.7.7 0 0 1-.7.7H8a.7.7 0 0 1-.7-.7V6.2a.7.7 0 0 1 .7-.7zM14.2 5.5H16a.7.7 0 0 1 .7.7v11.6a.7.7 0 0 1-.7.7h-1.8a.7.7 0 0 1-.7-.7V6.2a.7.7 0 0 1 .7-.7z"
            }
            Glyph::Next => {
                "M5.5 6.4v11.2a.5.5 0 0 0 .8.4l8-5.6a.5.5 0 0 0 0-.8l-8-5.6a.5.5 0 0 0-.8.4zM18.5 6v12"
            }
            Glyph::Previous => {
                "M18.5 6.4v11.2a.5.5 0 0 1-.8.4l-8-5.6a.5.5 0 0 1 0-.8l8-5.6a.5.5 0 0 1 .8.4zM5.5 6v12"
            }
            Glyph::Speaker => {
                "M4.5 9.5h2.7L12 5.6v12.8l-4.8-3.9H4.5a1 1 0 0 1-1-1v-3a1 1 0 0 1 1-1zM15.2 9.2a4 4 0 0 1 0 5.6M18 6.6a8 8 0 0 1 0 10.8"
            }
            Glyph::SpeakerLow => {
                "M4.5 9.5h2.7L12 5.6v12.8l-4.8-3.9H4.5a1 1 0 0 1-1-1v-3a1 1 0 0 1 1-1zM15.2 9.2a4 4 0 0 1 0 5.6"
            }
            Glyph::Muted => {
                "M4.5 9.5h2.7L12 5.6v12.8l-4.8-3.9H4.5a1 1 0 0 1-1-1v-3a1 1 0 0 1 1-1zM16 9.6l5 5M21 9.6l-5 5"
            }
            Glyph::Gear => {
                "M9.87 5.02L10.5 2.52L13.5 2.52L14.13 5.02L15.43 5.55L17.64 4.23L19.77 6.36L18.45 8.57L18.98 9.87L21.48 10.5L21.48 13.5L18.98 14.13L18.45 15.43L19.77 17.64L17.64 19.77L15.43 18.45L14.13 18.98L13.5 21.48L10.5 21.48L9.87 18.98L8.57 18.45L6.36 19.77L4.23 17.64L5.55 15.43L5.02 14.13L2.52 13.5L2.52 10.5L5.02 9.87L5.55 8.57L4.23 6.36L6.36 4.23L8.57 5.55zM12 9.2a2.8 2.8 0 1 0 0 5.6a2.8 2.8 0 1 0 0-5.6z"
            }
            Glyph::Sun => {
                "M12 8.4a3.6 3.6 0 1 0 0 7.2a3.6 3.6 0 1 0 0-7.2zM12 8.4a3.6 3.6 0 0 1 0 7.2zM12 3v1.8M12 19.2V21M3 12h1.8M19.2 12H21M5.6 5.6l1.3 1.3M17.1 17.1l1.3 1.3M5.6 18.4l1.3-1.3M17.1 6.9l1.3-1.3"
            }
            Glyph::Clear => {
                "M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8zM12 2.8v2M12 19.2v2M2.8 12h2M19.2 12h2M5.5 5.5l1.4 1.4M17.1 17.1l1.4 1.4M5.5 18.5l1.4-1.4M17.1 6.9l1.4-1.4"
            }
            Glyph::PartlyCloudy => {
                "M5.6 11.6A3.4 3.4 0 1 1 11.3 9.2M8 2.6v1.4M2.6 8h1.4M4.2 4.2l1 1M11.8 4.2l-1 1M9.5 20.5h8a3.2 3.2 0 0 0 .5-6.36A4.7 4.7 0 0 0 9.8 13.3 3.6 3.6 0 0 0 9.5 20.5z"
            }
            Glyph::Cloud => {
                "M7 19h10.5a3.6 3.6 0 0 0 .5-7.15A5.7 5.7 0 0 0 7.2 10.4 4.3 4.3 0 0 0 7 19z"
            }
            Glyph::Fog => "M5 7h11M3.5 11h16M6 15h14M3.5 19h9",
            Glyph::Drizzle => {
                "M7 14.5h9.5a3 3 0 0 0 .4-5.97A4.6 4.6 0 0 0 8.4 7.9 3.3 3.3 0 0 0 7 14.5zM9.5 17.5l-.6 1.8M14.5 17.5l-.6 1.8"
            }
            Glyph::Rain => {
                "M7 14.5h9.5a3 3 0 0 0 .4-5.97A4.6 4.6 0 0 0 8.4 7.9 3.3 3.3 0 0 0 7 14.5zM8.5 17.5l-1 2.8M12.5 17.5l-1 2.8M16.5 17.5l-1 2.8"
            }
            Glyph::Snow => {
                "M7 14.5h9.5a3 3 0 0 0 .4-5.97A4.6 4.6 0 0 0 8.4 7.9 3.3 3.3 0 0 0 7 14.5zM8 18.6h.01M12 20.6h.01M16 18.6h.01M10 21h.01"
            }
            Glyph::Storm => {
                "M7 14.5h9.5a3 3 0 0 0 .4-5.97A4.6 4.6 0 0 0 8.4 7.9 3.3 3.3 0 0 0 7 14.5zM12.8 15.6l-2.6 3.2h3l-2 3.2"
            }
            Glyph::Prayer => {
                "M5.5 20.5v-4.6a6.5 6.5 0 0 1 13 0v4.6zM13.4 2.8a2.8 2.8 0 1 0 0 5.2a2.2 2.2 0 1 1 0-5.2zM10.6 20.5v-2.4a1.4 1.4 0 0 1 2.8 0v2.4"
            }
            Glyph::Timer => {
                "M12 6.2a7.4 7.4 0 1 0 0 14.8a7.4 7.4 0 1 0 0-14.8zM12 13.6V9.9M9.6 2.8h4.8M18.4 6.2l1.3-1.3"
            }
            Glyph::Shelf => {
                "M3.5 13.8l2.1-6.9a1.6 1.6 0 0 1 1.5-1.1h9.8a1.6 1.6 0 0 1 1.5 1.1l2.1 6.9v4.2a1.6 1.6 0 0 1-1.6 1.6H5.1A1.6 1.6 0 0 1 3.5 18zM3.5 13.8h4.6a3.9 3.9 0 0 0 7.8 0h4.6"
            }
            Glyph::Start => {
                "M5 5h5.5v5.5H5zM13.5 5H19v5.5h-5.5zM5 13.5h5.5V19H5zM13.5 13.5H19V19h-5.5z"
            }
            Glyph::Apps => "M4 6h16v12H4zM4 10h16M9 10v8",
            Glyph::Limits => "M4 18a9 9 0 1 1 16 0M12 13l4-5M6 14H4M18 14h2M7 7l1.4 1.4M12 4v2",
            Glyph::Search => "M10.5 4a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13zM15.4 15.4L20.5 20.5",
            Glyph::TaskView => {
                "M9 7.5h9a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2zM4 15V7a2 2 0 0 1 2-2h9"
            }
            Glyph::Chevron => "M6 15l6-6 6 6",
            Glyph::Expand => "M6 9l6 6 6-6",
            Glyph::Desktop => "M3 5h18v12H3zM8 21h8M12 17v4",
            Glyph::Charging => "M13 2L5 13h6l-1 9 9-12h-6z",
            Glyph::Add => "M12 4v16M4 12h16",
            Glyph::More => "M5 12h.01M12 12h.01M19 12h.01",
            Glyph::File => "M7 3.5h7l4 4v13H7zM14 3.5v4h4",
        }
    }
}

/// One path command, absolute, on the grid. `A` is radius, large-arc, clockwise, then the end point.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Seg {
    M(f32, f32),
    L(f32, f32),
    A(f32, bool, bool, f32, f32),
    Z,
}

struct Scan<'a> {
    text: &'a [u8],
    at: usize,
}
impl Scan<'_> {
    fn skip(&mut self) {
        while self
            .text
            .get(self.at)
            .is_some_and(|&b| b == b' ' || b == b',')
        {
            self.at += 1;
        }
    }
    fn number_next(&mut self) -> bool {
        self.skip();
        self.text
            .get(self.at)
            .is_some_and(|&b| b == b'-' || b == b'.' || b.is_ascii_digit())
    }
    fn number(&mut self) -> f32 {
        self.skip();
        let start = self.at;
        if self.text.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        while self.text.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        if self.text.get(self.at) == Some(&b'.') {
            self.at += 1;
            while self.text.get(self.at).is_some_and(u8::is_ascii_digit) {
                self.at += 1;
            }
        }
        std::str::from_utf8(&self.text[start..self.at])
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or(0.0)
    }
}

/// Walks SVG path data (the `M L H V A Z` subset our glyphs use, arcs circular) as absolute segments.
fn parse(path: &str, mut emit: impl FnMut(Seg)) {
    let mut scan = Scan {
        text: path.as_bytes(),
        at: 0,
    };
    let (mut x, mut y, mut start) = (0.0, 0.0, (0.0, 0.0));
    let mut command = b'M';
    loop {
        scan.skip();
        let Some(&byte) = scan.text.get(scan.at) else {
            return;
        };
        if byte.is_ascii_alphabetic() {
            command = byte;
            scan.at += 1;
        } else if !scan.number_next() {
            return;
        }
        let (dx, dy) = if command.is_ascii_lowercase() {
            (x, y)
        } else {
            (0.0, 0.0)
        };
        match command.to_ascii_uppercase() {
            b'Z' => {
                emit(Seg::Z);
                (x, y) = start;
                continue;
            }
            b'H' => x = dx + scan.number(),
            b'V' => y = dy + scan.number(),
            b'A' => {
                let radius = scan.number();
                scan.number();
                scan.number();
                let (large, sweep) = (scan.number() != 0.0, scan.number() != 0.0);
                (x, y) = (dx + scan.number(), dy + scan.number());
                emit(Seg::A(radius, large, sweep, x, y));
                continue;
            }
            _ => (x, y) = (dx + scan.number(), dy + scan.number()),
        }
        if command.eq_ignore_ascii_case(&b'M') {
            start = (x, y);
            emit(Seg::M(x, y));
            command = if command == b'M' { b'L' } else { b'l' };
        } else {
            emit(Seg::L(x, y));
        }
    }
}

/// The stroke for an icon `side` physical pixels wide: 1.75 grid units snapped to whole device pixels (at
/// least one), so axis-aligned lines are crisp at 100, 125, 150 and 200 percent.
fn stroke_pixels(side: f32) -> f32 {
    (STROKE * side / GRID).round().max(1.0)
}

fn snap(value: f32, side: f32) -> f32 {
    let phase = (stroke_pixels(side) % 2.0) / 2.0;
    (value * side / GRID - phase).round() + phase
}

/// Geometry and stroke style for one factory; rebuilt when the device (and its factory) is.
struct Kit {
    factory: usize,
    style: ID2D1StrokeStyle,
    shapes: [Vec<(f32, ID2D1PathGeometry)>; ALL.len()],
}
thread_local! {
    static KIT: RefCell<Option<Kit>> = const { RefCell::new(None) };
}

fn stroke_style(factory: &ID2D1Factory) -> Result<ID2D1StrokeStyle> {
    // SAFETY: the factory is live and copies this local stroke descriptor before returning.
    unsafe {
        factory.CreateStrokeStyle(
            &D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 1.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
            },
            None,
        )
    }
}

fn build(factory: &ID2D1Factory, glyph: Glyph, side: f32) -> Result<ID2D1PathGeometry> {
    // SAFETY: the geometry and sink are local COM objects; the sink is closed before the geometry is used.
    unsafe {
        let shape = factory.CreatePathGeometry()?;
        let sink = shape.Open()?;
        let mut open = false;
        parse(glyph.path(), |segment| {
            let point = |x, y| Vector2 {
                X: snap(x, side),
                Y: snap(y, side),
            };
            match segment {
                Seg::M(x, y) => {
                    if open {
                        sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    }
                    sink.BeginFigure(point(x, y), D2D1_FIGURE_BEGIN_HOLLOW);
                    open = true;
                }
                Seg::L(x, y) => sink.AddLine(point(x, y)),
                Seg::A(radius, large, sweep, x, y) => sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: point(x, y),
                    size: D2D_SIZE_F {
                        width: radius * side / GRID,
                        height: radius * side / GRID,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: if sweep {
                        D2D1_SWEEP_DIRECTION_CLOCKWISE
                    } else {
                        D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                    },
                    arcSize: if large {
                        D2D1_ARC_SIZE_LARGE
                    } else {
                        D2D1_ARC_SIZE_SMALL
                    },
                }),
                Seg::Z => {
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    open = false;
                }
            }
        });
        if open {
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
        }
        sink.Close()?;
        Ok(shape)
    }
}

/// Strokes `glyph` in `square` (physical pixels) in `color`.
pub(super) fn draw(
    context: &ID2D1DeviceContext,
    square: D2D_RECT_F,
    glyph: Glyph,
    color: D2D1_COLOR_F,
) -> Result<()> {
    let side = square.right - square.left;
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is local, the cached geometry
    // and style belong to this thread's factory, and the transform is put back before returning.
    unsafe {
        let factory = context.GetFactory()?;
        let key = windows::core::Interface::as_raw(&factory) as usize;
        let (shape, style) = KIT.with(|kit| -> Result<_> {
            let mut kit = kit.borrow_mut();
            if kit.as_ref().is_none_or(|held| held.factory != key) {
                let style = stroke_style(&factory)?;
                *kit = Some(Kit {
                    factory: key,
                    style,
                    shapes: std::array::from_fn(|_| Vec::new()),
                });
            }
            let kit = kit.as_mut().expect("set above");
            let slot = &mut kit.shapes[glyph as usize];
            if let Some((_, shape)) = slot.iter().find(|(size, _)| *size == side) {
                return Ok((shape.clone(), kit.style.clone()));
            }
            let shape = build(&factory, glyph, side)?;
            if slot.len() == 4 {
                slot.remove(0);
            }
            slot.push((side, shape.clone()));
            Ok((shape, kit.style.clone()))
        })?;
        let brush = context.CreateSolidColorBrush(&color, None)?;
        let mut before = Matrix3x2::identity();
        context.GetTransform(&mut before);
        context.SetTransform(&Matrix3x2 {
            M11: 1.0,
            M12: 0.0,
            M21: 0.0,
            M22: 1.0,
            M31: square.left.round(),
            M32: square.top.round(),
        });
        context.DrawGeometry(&shape, &brush, stroke_pixels(side), Some(&style));
        context.SetTransform(&before);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_ink_stays_visible_when_high_contrast_makes_the_wash_opaque() {
        let palette = crate::theme::Theme::Classic.palette();
        assert_eq!(active_ink(&palette), palette.accent);
        let contrast = Palette {
            high_contrast: Some(((128, 128, 128), (128, 128, 128))),
            ..palette
        };
        assert_eq!(active_ink(&contrast), contrast.on_ink);
        assert_ne!(active_ink(&contrast), contrast.accent);
    }

    #[test]
    fn geometry_bounds_include_arcs_and_strokes_at_every_scale() {
        use windows::Win32::Graphics::Direct2D::{
            D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory,
        };
        // SAFETY: local offscreen factory/geometries only; no HWND, input, or desktop state is used.
        unsafe {
            let factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).unwrap();
            let style = stroke_style(&factory).unwrap();
            for glyph in ALL {
                for dips in [16.0, 20.0, 24.0, 32.0] {
                    for scale in [1.0, 1.25, 1.5, 2.0] {
                        let side = (dips * scale as f32).round();
                        let shape = build(&factory, glyph, side).unwrap();
                        let bounds = shape
                            .GetWidenedBounds(stroke_pixels(side), Some(&style), None, 0.01)
                            .unwrap();
                        assert!(
                            bounds.left <= bounds.right && bounds.top <= bounds.bottom,
                            "{glyph:?}: empty geometry"
                        );
                        assert!(
                            bounds.left >= -0.1
                                && bounds.top >= -0.1
                                && bounds.right <= side + 0.1
                                && bounds.bottom <= side + 0.1,
                            "{glyph:?} at {side}: {bounds:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_glyph_parses_and_stays_inside_the_grid() {
        for (index, glyph) in ALL.into_iter().enumerate() {
            assert_eq!(glyph as usize, index, "{glyph:?} is out of order in ALL");
            let mut count = 0;
            parse(glyph.path(), |segment| {
                count += 1;
                if let Seg::M(x, y) | Seg::L(x, y) | Seg::A(_, _, _, x, y) = segment {
                    assert!(
                        (0.0..=GRID).contains(&x) && (0.0..=GRID).contains(&y),
                        "{glyph:?} leaves the grid at {x} {y}"
                    );
                }
            });
            assert!(count >= 2, "{glyph:?} drew nothing");
        }
    }

    #[test]
    fn the_parser_reads_relative_arcs_implicit_lines_and_closes() {
        let mut got = Vec::new();
        parse("M1 2l.5-1H4v2a1 1 0 0 1-1 1zm1 1 2 2", |s| got.push(s));
        assert_eq!(
            got,
            [
                Seg::M(1.0, 2.0),
                Seg::L(1.5, 1.0),
                Seg::L(4.0, 1.0),
                Seg::L(4.0, 3.0),
                Seg::A(1.0, false, true, 3.0, 4.0),
                Seg::Z,
                Seg::M(2.0, 3.0),
                Seg::L(4.0, 5.0),
            ]
        );
    }

    #[test]
    fn strokes_snap_to_whole_pixels_at_every_size_and_scale() {
        for dips in [16.0, 20.0, 24.0, 32.0] {
            for scale in [1.0, 1.25, 1.5, 2.0] {
                let side = (dips * scale as f32).round();
                let width = stroke_pixels(side);
                assert_eq!(width, width.round());
                for coordinate in [2.8, 4.0, 8.5, 12.0, 20.0, 21.2] {
                    let centre = snap(coordinate, side);
                    assert_eq!((centre - width / 2.0).fract(), 0.0);
                    assert_eq!((centre + width / 2.0).fract(), 0.0);
                }
                assert!((1.0..=side / 8.0 + 1.0).contains(&width), "{side}: {width}");
            }
        }
        assert_eq!(stroke_pixels(16.0), 1.0);
        assert_eq!(stroke_pixels(24.0), 2.0);
        assert_eq!(stroke_pixels(64.0), 5.0);
    }
}
