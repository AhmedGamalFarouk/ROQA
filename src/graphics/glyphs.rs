//! Every drawn icon (features I1, direction B "crisp line"): one 24-unit grid, 1.75-unit strokes with round
//! caps and joins, no fills, one `DrawGeometry` per icon. Each glyph is one SVG-style path string; the cached
//! geometry is built once per Direct2D factory, and the stroke is snapped to whole device pixels so lines
//! stay sharp at any DPI. No raster, no icon font.

use super::*;
use crate::weather::Icon;
use std::cell::RefCell;
use windows::Win32::Graphics::Direct2D::{
    Common::{D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN},
    D2D1_ARC_SIZE_LARGE, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_LINE_JOIN_ROUND, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_STROKE_STYLE_PROPERTIES, D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory,
    ID2D1RenderTarget, ID2D1StrokeStyle,
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
    /// A grid of nine dots: the stand-in for a folder tile's missing app icon.
    Apps,
    Search,
    TaskView,
    Chevron,
    Expand,
    Charging,
    Add,
    More,
    /// A shelf entry whose system icon did not load.
    File,
    /// The Media tab: a note.
    Music,
    /// The Widgets tab: a tall and two small tiles.
    Widgets,
    Close,
    /// Settings' Notch page: the notch hanging from the screen edge, with its eyes.
    Notch,
    /// Settings' About page.
    Info,
    /// The dock's Places button: a hard drive.
    Drive,
    /// The dock's hidden-icons button: a chevron over three dots.
    Tray,
    /// Settings' Hotkeys page: a keyboard.
    Keys,
    /// The month page's arrows: a chevron left and right.
    Back,
    Forward,
    /// The Music page's mode chips: two arrows crossing, a loop of two arrows, and the loop with a 1.
    Shuffle,
    Repeat,
    RepeatOne,
}
const ALL: [Glyph; 43] = [
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
    Glyph::Apps,
    Glyph::Search,
    Glyph::TaskView,
    Glyph::Chevron,
    Glyph::Expand,
    Glyph::Charging,
    Glyph::Add,
    Glyph::More,
    Glyph::File,
    Glyph::Music,
    Glyph::Widgets,
    Glyph::Close,
    Glyph::Notch,
    Glyph::Info,
    Glyph::Drive,
    Glyph::Tray,
    Glyph::Keys,
    Glyph::Back,
    Glyph::Forward,
    Glyph::Shuffle,
    Glyph::Repeat,
    Glyph::RepeatOne,
];
/// Grid size and stroke, in grid units.
const GRID: f32 = 24.0;
// Normalize the 24-unit source paths to the brand 20-unit optical grid.
const STROKE: f32 = 1.75 * GRID / 20.0;

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
    /// The tab's icon on the handle: the one stroke style, the accent when selected.
    pub(super) fn tab(tab: crate::tabs::Tab) -> Glyph {
        use crate::tabs::Tab;
        match tab {
            Tab::Media => Glyph::Music,
            Tab::Weather => Glyph::PartlyCloudy,
            Tab::Prayer => Glyph::Prayer,
            Tab::Timer => Glyph::Timer,
            Tab::Shelf => Glyph::Shelf,
            Tab::Limits => Glyph::Limits,
            Tab::Widgets => Glyph::Widgets,
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
            Glyph::Close => "M7 7l10 10M17 7L7 17",
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
                "M3 7h3M10 7h11M3 17h11M18 17h3M8 5a2 2 0 1 0 0 4a2 2 0 1 0 0-4zM16 15a2 2 0 1 0 0 4a2 2 0 1 0 0-4z"
            }
            // The brightness sun and the clear sky are one tracing: rays long enough to read at 16 px.
            Glyph::Sun | Glyph::Clear => {
                "M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8zM12 2.8v2M12 19.2v2M2.8 12h2M19.2 12h2M5.5 5.5l1.4 1.4M17.1 17.1l1.4 1.4M5.5 18.5l1.4-1.4M17.1 6.9l1.4-1.4"
            }
            Glyph::PartlyCloudy => {
                "M5.6 11.6A3.4 3.4 0 1 1 11.3 9.2M9.5 20.5h8a3.2 3.2 0 0 0 .5-6.36A4.7 4.7 0 0 0 9.8 13.3 3.6 3.6 0 0 0 9.5 20.5z"
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
            Glyph::Prayer => "M3 18h18M6 18a6 6 0 0 1 12 0M12 4v2M4.5 9l1.5 1.5M19.5 9L18 10.5",
            Glyph::Timer => "M12 6a7.5 7.5 0 1 0 0 15a7.5 7.5 0 1 0 0-15zM12 10v4l3 2M9 2.5h6",
            Glyph::Shelf => {
                "M3.5 15h17v4a1.5 1.5 0 0 1-1.5 1.5H5a1.5 1.5 0 0 1-1.5-1.5zM12 3v11M7.5 9.5L12 14l4.5-4.5"
            }
            Glyph::Apps => {
                "M6 4h2a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM16 4h2a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2h-2a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM6 14h2a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-2a2 2 0 0 1 2-2zM16 14h2a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2h-2a2 2 0 0 1-2-2v-2a2 2 0 0 1 2-2z"
            }
            Glyph::Limits => "M3.5 18a8.5 8.5 0 1 1 17 0M12 15l5-6",
            Glyph::Search => "M10.5 4a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13zM15.4 15.4L20.5 20.5",
            Glyph::TaskView => {
                "M10 9h8a2 2 0 0 1 2 2v5.5a2 2 0 0 1-2 2h-8a2 2 0 0 1-2-2V11a2 2 0 0 1 2-2zM5 14.5V8a2 2 0 0 1 2-2h8"
            }
            Glyph::Chevron => "M6 15l6-6 6 6",
            Glyph::Expand => "M6 9l6 6 6-6",
            Glyph::Back => "M15 6l-6 6 6 6",
            Glyph::Forward => "M9 6l6 6-6 6",
            Glyph::Shuffle => "M4 7h3l10 10h3M17 14l3 3l-3 3M4 17h3l3-3M14 10l3-3h3M17 4l3 3l-3 3",
            Glyph::Repeat => {
                "M17 3l3 3l-3 3M20 6H8a4 4 0 0 0-4 4v1M7 21l-3-3l3-3M4 18h12a4 4 0 0 0 4-4v-1"
            }
            Glyph::RepeatOne => {
                "M17 3l3 3l-3 3M20 6H8a4 4 0 0 0-4 4v1M7 21l-3-3l3-3M4 18h12a4 4 0 0 0 4-4v-1M11 10.5l1.5-1.2V15"
            }
            Glyph::Charging => "M13 2L5 13h6l-1 9 9-12h-6z",
            Glyph::Add => "M12 4v16M4 12h16",
            Glyph::More => "M5 12h.01M12 12h.01M19 12h.01",
            Glyph::File => {
                "M7 3.5h6l5 5v10a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-13a2 2 0 0 1 2-2zM13 3.5v5h5"
            }
            Glyph::Music => {
                "M7.5 4h9A3.5 3.5 0 0 1 20 7.5v9a3.5 3.5 0 0 1-3.5 3.5h-9A3.5 3.5 0 0 1 4 16.5v-9A3.5 3.5 0 0 1 7.5 4zM10.5 9.2l4.2 2.8-4.2 2.8z"
            }
            Glyph::Notch => {
                "M3 4.5h18M6.5 4.5V8a5.5 5.5 0 0 0 5.5 5.5 5.5 5.5 0 0 0 5.5-5.5V4.5M10 8.2v.6M14 8.2v.6"
            }
            Glyph::Info => "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18zM12 11v5.5M12 7.8h.01",
            Glyph::Drive => {
                "M4 14.5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2zM4 14.5l2.3-7.6A2 2 0 0 1 8.2 5.5h7.6a2 2 0 0 1 1.9 1.4l2.3 7.6M16.5 16h.01"
            }
            Glyph::Tray => "M8 11l4-4 4 4M7 17h.01M12 17h.01M17 17h.01",
            Glyph::Keys => {
                "M6 6.5h12a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2zM8 10.2h.01M12 10.2h.01M16 10.2h.01M8.5 14h7"
            }
            Glyph::Widgets => {
                "M5.5 4h3.5a1.5 1.5 0 0 1 1.5 1.5v13a1.5 1.5 0 0 1-1.5 1.5H5.5A1.5 1.5 0 0 1 4 18.5v-13A1.5 1.5 0 0 1 5.5 4zM15 4h3.5A1.5 1.5 0 0 1 20 5.5V9a1.5 1.5 0 0 1-1.5 1.5H15A1.5 1.5 0 0 1 13.5 9V5.5A1.5 1.5 0 0 1 15 4zM15 13.5h3.5a1.5 1.5 0 0 1 1.5 1.5v3.5a1.5 1.5 0 0 1-1.5 1.5H15a1.5 1.5 0 0 1-1.5-1.5V15a1.5 1.5 0 0 1 1.5-1.5z"
            }
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
    // Two factories live on the host thread: the dock device's and the Settings DC target's.
    static KIT: RefCell<Vec<Kit>> = const { RefCell::new(Vec::new()) };
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
    context: &ID2D1RenderTarget,
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
            let mut kits = kit.borrow_mut();
            let at = match kits.iter().position(|held| held.factory == key) {
                Some(at) => at,
                None => {
                    let style = stroke_style(&factory)?;
                    kits.push(Kit {
                        factory: key,
                        style,
                        shapes: std::array::from_fn(|_| Vec::new()),
                    });
                    kits.len() - 1
                }
            };
            let kit = &mut kits[at];
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

thread_local! {
    static DC_TARGET: RefCell<Option<ID2D1DCRenderTarget>> = const { RefCell::new(None) };
}

/// Runs `f` on an anti-aliased Direct2D target bound to `rect` of a GDI DC (Settings paints with GDI text
/// and this for every shape); (0, 0) is the rect's top left, units are physical pixels.
fn with_dc(
    dc: windows::Win32::Graphics::Gdi::HDC,
    rect: windows::Win32::Foundation::RECT,
    f: impl FnOnce(&ID2D1RenderTarget) -> Result<()>,
) {
    let run = || -> Result<()> {
        let target = DC_TARGET.with(|held| -> Result<_> {
            let mut held = held.borrow_mut();
            if held.is_none() {
                // SAFETY: a single-threaded factory used only on the host thread that owns Settings.
                let factory: ID2D1Factory =
                    unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
                let props = D2D1_RENDER_TARGET_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    // 96 dpi: coordinates are physical pixels, as the GDI callers already compute them.
                    dpiX: 96.0,
                    dpiY: 96.0,
                    ..Default::default()
                };
                // SAFETY: plain value properties.
                *held = Some(unsafe { factory.CreateDCRenderTarget(&props)? });
            }
            Ok(held.clone().expect("set above"))
        })?;
        // SAFETY: the caller's DC is live; BeginDraw/EndDraw bracket the drawing.
        unsafe {
            target.BindDC(dc, &rect)?;
            target.BeginDraw();
            let result = f(&target);
            target.EndDraw(None, None)?;
            result
        }
    };
    let _ = run();
}

fn bgr(value: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: (value & 0xff) as f32 / 255.0,
        g: (value >> 8 & 0xff) as f32 / 255.0,
        b: (value >> 16 & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

/// Settings glyph on its GDI DC: the same paths and rounded stroke as the D2D tabs, anti-aliased. `ink` is a COLORREF.
pub fn draw_gdi(
    dc: windows::Win32::Graphics::Gdi::HDC,
    rect: windows::Win32::Foundation::RECT,
    glyph: Glyph,
    ink: u32,
) {
    let side = (rect.right - rect.left) as f32;
    let square = D2D_RECT_F {
        left: 0.0,
        top: 0.0,
        right: side,
        bottom: side,
    };
    with_dc(dc, rect, |target| draw(target, square, glyph, bgr(ink)));
}

/// A Settings capsule or card on its GDI DC, anti-aliased: `fill` and `line` are COLORREFs, `line == fill` means no
/// outline. Corner radius `radius` px.
pub fn round_gdi(
    dc: windows::Win32::Graphics::Gdi::HDC,
    rect: windows::Win32::Foundation::RECT,
    radius: i32,
    fill: u32,
    line: u32,
) {
    let (w, h) = (
        (rect.right - rect.left) as f32,
        (rect.bottom - rect.top) as f32,
    );
    with_dc(dc, rect, |target| {
        let shape = |inset: f32| D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: inset,
                top: inset,
                right: w - inset,
                bottom: h - inset,
            },
            radiusX: (radius as f32 - inset).min(w / 2.0).max(0.0),
            radiusY: (radius as f32 - inset).min(h / 2.0).max(0.0),
        };
        // SAFETY: the target is live between BeginDraw/EndDraw; the brushes are local.
        unsafe {
            if fill != u32::MAX {
                target.FillRoundedRectangle(
                    &shape(0.0),
                    &target.CreateSolidColorBrush(&bgr(fill), None)?,
                );
            }
            if line != fill {
                target.DrawRoundedRectangle(
                    &shape(0.5),
                    &target.CreateSolidColorBrush(&bgr(line), None)?,
                    1.0,
                    None,
                );
            }
        }
        Ok(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(stroke_pixels(64.0), 6.0);
    }
}
