//! Edge sliders (features B2.5): volume on each monitor's right edge, brightness on its left. A thin
//! invisible non-activating strip in the middle third of the edge waits for the pointer; after a short
//! hover the same window grows into a small pill you drag (or scroll) to set the level, and shrinks back
//! to the strip after the pointer leaves. Clicking the volume icon lists the output devices.
//!
//! Everything is event-driven: mouse messages feed `Hover`, whose two deadlines are the only waits, and a
//! pill is painted only when what it shows changes. The pill's canvas exists only while shown.
use crate::{
    audio::{Audio, Device},
    brightness::{Brightness, Target},
    dock::px,
    dock_uia::Accessibility,
    edge_uia,
    graphics::{Canvas, Glyph, Graphics, ListScene, SliderScene},
    layout::Bounds,
    native_window::NativeWindow,
    policy_config,
    settings::Settings,
    theme::Palette,
    topology::Monitor,
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::LPARAM,
        System::Threading::GetCurrentThreadId,
        UI::{
            Accessibility::UiaClientsAreListening,
            Controls::WM_MOUSELEAVE,
            Input::KeyboardAndMouse::{
                ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
            },
            Shell::ShellExecuteW,
            WindowsAndMessaging::{
                MSG, SW_SHOWNORMAL, WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
                WM_MOUSEWHEEL,
            },
        },
    },
    core::{PCWSTR, Result, w},
};

/// The pointer must rest this long on the strip before the pill shows, and be away this long before it hides.
pub const SHOW_DELAY: Duration = Duration::from_millis(250);
pub const HIDE_DELAY: Duration = Duration::from_millis(800);
/// Brightness writes go out at most this often (about 5 a second); the latest value wins.
pub const WRITE_GAP: Duration = Duration::from_millis(200);

// Geometry in DIPs.
const STRIP: i32 = 2;
const PILL_W: i32 = 44;
const PILL_H: i32 = 200;
const MARGIN: i32 = 8;
const NOTE_W: i32 = 190;
const LIST_W: i32 = 250;
const ROW_H: i32 = 34;
const LIST_PAD: i32 = 6;
/// Percent per wheel notch.
const WHEEL_STEP: i32 = 5;
const WHEEL_DELTA: i32 = 120;
const MK_LBUTTON: usize = 0x0001;
/// The text drawn for a monitor that does not answer DDC/CI.
pub const UNSUPPORTED: &str = "Brightness not supported";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Volume,
    Brightness,
}
impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Volume, Kind::Brightness];
    /// Volume sits on the right edge, brightness on the left.
    pub fn right(self) -> bool {
        self == Kind::Volume
    }
    fn title(self) -> PCWSTR {
        match self {
            Kind::Volume => w!("ROQA Volume Slider"),
            Kind::Brightness => w!("ROQA Brightness Slider"),
        }
    }
    fn id(self) -> &'static str {
        match self {
            Kind::Volume => "RoqaVolumeSlider",
            Kind::Brightness => "RoqaBrightnessSlider",
        }
    }
    fn enabled(self, settings: &Settings) -> bool {
        match self {
            Kind::Volume => settings.edge_volume,
            Kind::Brightness => settings.edge_brightness,
        }
    }
}

// ---- Geometry (pure) -------------------------------------------------------------------------------

/// The hot strip on `kind`'s edge: the middle third of the work area vertically, so scrollbars and window
/// edges near the corners keep working.
pub fn strip(work: Bounds, dpi: u32, kind: Kind) -> Bounds {
    let width = px(STRIP, dpi).max(2);
    Bounds {
        x: if kind.right() {
            work.x + work.width - width
        } else {
            work.x
        },
        y: work.y + work.height / 3,
        width,
        height: work.height / 3,
    }
}

/// A window of `size` (px) at `kind`'s edge, centred vertically in the work area and kept inside it.
pub fn place(work: Bounds, kind: Kind, size: (i32, i32)) -> Bounds {
    let (width, height) = (size.0.min(work.width), size.1.min(work.height));
    Bounds {
        x: if kind.right() {
            work.x + work.width - width
        } else {
            work.x
        },
        y: work.y + (work.height - height) / 2,
        width,
        height,
    }
}

/// Positions inside the pill window, in window pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub size: (i32, i32),
    pub pill: Bounds,
    pub track: Bounds,
    pub icon: Bounds,
    /// The percent inside the pill, or (wide) the note beside it.
    pub caption: Bounds,
}

/// The pill's layout; `wide` adds room beside the pill for a note.
pub fn layout(dpi: u32, kind: Kind, wide: bool) -> Layout {
    let d = |dips| px(dips, dpi);
    let (pill_w, pill_h, margin) = (d(PILL_W), d(PILL_H), d(MARGIN));
    let width = pill_w + 2 * margin + if wide { d(NOTE_W) } else { 0 };
    let height = pill_h + 2 * margin;
    let pill_x = if wide && kind.right() {
        width - margin - pill_w
    } else {
        margin
    };
    let pill = Bounds {
        x: pill_x,
        y: margin,
        width: pill_w,
        height: pill_h,
    };
    let (pad, caption_h) = (d(6), d(22));
    let track_w = d(8);
    let icon_size = d(24);
    let icon = Bounds {
        x: pill.x + (pill_w - icon_size) / 2,
        y: pill.y + pill_h - d(10) - icon_size,
        width: icon_size,
        height: icon_size,
    };
    let track_top = pill.y + pad + caption_h + pad;
    let track = Bounds {
        x: pill.x + (pill_w - track_w) / 2,
        y: track_top,
        width: track_w,
        height: (icon.y - d(10) - track_top).max(1),
    };
    let caption = if wide {
        let x = if kind.right() {
            margin
        } else {
            pill.x + pill_w + d(8)
        };
        Bounds {
            x,
            y: pill.y + (pill_h - caption_h) / 2,
            width: d(NOTE_W) - d(8),
            height: caption_h,
        }
    } else {
        Bounds {
            x: pill.x,
            y: pill.y + pad,
            width: pill_w,
            height: caption_h,
        }
    };
    Layout {
        size: (width, height),
        pill,
        track,
        icon,
        caption,
    }
}

/// The level a pointer at window row `y` means: 100 at the top of the track, 0 at its bottom, clamped.
pub fn level_at(y: i32, layout: &Layout) -> u8 {
    let (top, height) = (layout.track.y, layout.track.height.max(1));
    let from_bottom = f64::from(top + height - y) / f64::from(height);
    (from_bottom * 100.0).round().clamp(0.0, 100.0) as u8
}

/// Whether the point is on the icon (with a little slack, since it is small).
pub fn on_icon(x: i32, y: i32, layout: &Layout, dpi: u32) -> bool {
    let slack = px(6, dpi);
    let icon = layout.icon;
    (icon.x - slack..icon.x + icon.width + slack).contains(&x)
        && (icon.y - slack..icon.y + icon.height + slack).contains(&y)
}

/// One row of the device list, in window pixels.
pub struct ListRow<'a> {
    pub name: &'a str,
    pub current: bool,
    pub rect: Bounds,
}

/// The device list window's size and its panel inside the transparent margin.
pub fn list_layout(dpi: u32, rows: usize) -> ((i32, i32), Bounds) {
    let margin = px(MARGIN, dpi);
    let panel = Bounds {
        x: margin,
        y: margin,
        width: px(LIST_W, dpi),
        height: rows as i32 * px(ROW_H, dpi) + 2 * px(LIST_PAD, dpi),
    };
    ((panel.width + 2 * margin, panel.height + 2 * margin), panel)
}

pub fn row_rect(dpi: u32, panel: Bounds, index: usize) -> Bounds {
    let pad = px(LIST_PAD, dpi);
    Bounds {
        x: panel.x + pad,
        y: panel.y + pad + index as i32 * px(ROW_H, dpi),
        width: panel.width - 2 * pad,
        height: px(ROW_H, dpi),
    }
}

/// The row under window point `(x, y)`.
pub fn row_at(dpi: u32, rows: usize, x: i32, y: i32) -> Option<usize> {
    let (_, panel) = list_layout(dpi, rows);
    (0..rows).find(|&index| {
        let row = row_rect(dpi, panel, index);
        (row.x..row.x + row.width).contains(&x) && (row.y..row.y + row.height).contains(&y)
    })
}

/// Wheel movement `delta` added to the remainder `acc` of earlier ones: whole notches, and what is left.
pub fn wheel_steps(acc: i32, delta: i32) -> (i32, i32) {
    let total = acc + delta;
    (total / WHEEL_DELTA, total % WHEEL_DELTA)
}

pub fn step_level(level: u8, steps: i32) -> u8 {
    (i32::from(level) + steps * WHEEL_STEP).clamp(0, 100) as u8
}

// ---- Timing (pure) ---------------------------------------------------------------------------------

/// When the pill shows and hides: `SHOW_DELAY` after the pointer arrives, `HIDE_DELAY` after it leaves
/// (never while a drag holds it).
#[derive(Default)]
pub struct Hover {
    over: bool,
    shown: bool,
    due: Option<Instant>,
}
impl Hover {
    pub fn enter(&mut self, now: Instant) {
        if !std::mem::replace(&mut self.over, true) {
            self.due = (!self.shown).then_some(now + SHOW_DELAY);
        }
    }
    pub fn leave(&mut self, now: Instant) {
        if std::mem::replace(&mut self.over, false) {
            self.due = self.shown.then_some(now + HIDE_DELAY);
        }
    }
    /// `Some(true)` when the pill is due to show, `Some(false)` when it is due to hide.
    pub fn tick(&mut self, now: Instant, held: bool) -> Option<bool> {
        self.due.filter(|&due| now >= due)?;
        if !self.shown {
            self.due = None;
            self.shown = true;
            return Some(true);
        }
        if held {
            self.due = Some(now + HIDE_DELAY);
            return None;
        }
        self.due = None;
        self.shown = false;
        Some(false)
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.due
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Coalesces brightness writes: the latest value waits for the gap since the last send.
#[derive(Default)]
pub struct Throttle {
    sent: Option<Instant>,
    latest: Option<u8>,
}
impl Throttle {
    pub fn push(&mut self, value: u8) {
        self.latest = Some(value);
    }
    /// The value to send now, if one waits and `WRITE_GAP` has passed since the last send.
    pub fn take(&mut self, now: Instant) -> Option<u8> {
        self.latest?;
        if self.sent.is_some_and(|sent| now < sent + WRITE_GAP) {
            return None;
        }
        self.sent = Some(now);
        self.latest.take()
    }
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.latest
            .map(|_| self.sent.map_or(now, |sent| sent + WRITE_GAP))
    }
}

// ---- The windows -----------------------------------------------------------------------------------

/// What the brightness slider knows of its monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reading {
    Pending,
    Level(u8),
    Unsupported,
}

enum Mode {
    Level,
    List {
        devices: Vec<Device>,
        hover: Option<usize>,
    },
}

/// Exactly what is drawn: a repaint happens only when it changes.
#[derive(PartialEq, Eq)]
enum View {
    Level {
        percent: u8,
        glyph: Glyph,
        dim: bool,
        caption: String,
        unsupported: bool,
    },
    List {
        hover: Option<usize>,
    },
}

struct Slider {
    // UIA is released first, then the canvas, then the window.
    accessibility: Accessibility,
    canvas: Option<Canvas>,
    window: NativeWindow,
    kind: Kind,
    monitor: String,
    work: Bounds,
    dpi: u32,
    hover: Hover,
    tracking: bool,
    /// Hidden while a full-screen app covers the monitor.
    suppressed: bool,
    mode: Mode,
    drawn: Option<View>,
    /// The level being dragged, shown instead of the device's until the drag ends.
    drag: Option<u8>,
    wheel: i32,
    /// The pointer has been on the strip, so the volume level is wanted.
    touched: bool,
    brightness: Reading,
    read_wanted: bool,
    throttle: Throttle,
    /// The default device's name, for the accessible name; `name_revision` bumps when it changes.
    device: Option<String>,
    name_revision: u32,
    published: Option<(u8, bool, bool, u32, bool, bool)>,
    uia: edge_uia::Shared,
}
impl Drop for Slider {
    fn drop(&mut self) {
        self.accessibility.disconnect();
        edge_uia::invalidate(&self.uia);
        if self.drag.is_some() {
            // SAFETY: releases this thread's own capture; harmless if it is already gone.
            let _ = unsafe { ReleaseCapture() };
        }
    }
}

fn point(lparam: LPARAM) -> (i32, i32) {
    (
        i32::from((lparam.0 & 0xffff) as u16 as i16),
        i32::from((lparam.0 >> 16) as u16 as i16),
    )
}

/// The Sound settings page: where an unavailable device switch sends the user.
fn open_sound_settings() {
    // SAFETY: a shell open of a fixed URI; the shell shows any failure itself.
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("ms-settings:sound"),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

impl Slider {
    fn create(kind: Kind, monitor: &Monitor) -> Result<Self> {
        let window =
            NativeWindow::create_hidden(strip(monitor.work, monitor.dpi, kind), kind.title())?;
        let uia: edge_uia::Shared = Arc::new(Mutex::new(edge_uia::State {
            id: kind.id(),
            ..Default::default()
        }));
        let accessibility = edge_uia::accessibility(window.0, &uia)?;
        window.show();
        Ok(Self {
            accessibility,
            canvas: None,
            window,
            kind,
            monitor: monitor.device.clone(),
            work: monitor.work,
            dpi: monitor.dpi,
            hover: Hover::default(),
            tracking: false,
            suppressed: false,
            mode: Mode::Level,
            drawn: None,
            drag: None,
            wheel: 0,
            touched: false,
            brightness: Reading::Pending,
            read_wanted: false,
            throttle: Throttle::default(),
            device: None,
            name_revision: 0,
            published: None,
            uia,
        })
    }
    fn target(&self) -> Target {
        (
            self.work.x + self.work.width / 2,
            self.work.y + self.work.height / 2,
        )
    }
    fn wide(&self) -> bool {
        self.kind == Kind::Brightness && self.brightness == Reading::Unsupported
    }
    fn layout(&self) -> Layout {
        layout(self.dpi, self.kind, self.wide())
    }
    /// The level shown: the drag's, else the device's.
    fn percent(&self, audio: &Audio) -> Option<u8> {
        match self.kind {
            Kind::Volume => self.drag.or(audio.level().map(|level| level.percent)),
            Kind::Brightness => match self.brightness {
                Reading::Level(percent) => Some(self.drag.unwrap_or(percent)),
                _ => None,
            },
        }
    }
    fn enabled(&self, audio: &Audio) -> bool {
        match self.kind {
            // Before Core Audio starts there is nothing to say against it.
            Kind::Volume => audio.level().is_some() || !audio.active(),
            Kind::Brightness => self.brightness != Reading::Unsupported,
        }
    }

    // Showing and hiding.

    fn open(&mut self, graphics: &Graphics, palette: Palette, audio: &Audio) {
        self.mode = Mode::Level;
        if self.kind == Kind::Volume {
            self.set_device(audio.default_name());
        } else {
            self.brightness = Reading::Pending;
            self.read_wanted = true;
        }
        self.show_pill(graphics, palette);
    }
    fn show_pill(&mut self, graphics: &Graphics, palette: Palette) {
        let shown = self.resize_to(self.layout().size).and_then(|surface| {
            if self.canvas.is_some() {
                return Ok(());
            }
            Canvas::create(graphics, self.window.0, surface, palette)
                .map(|canvas| self.canvas = Some(canvas))
        });
        self.drawn = None;
        if let Err(error) = shown {
            eprintln!("Edge slider: show: {error}");
            self.close();
        }
    }
    /// Back to the strip: no canvas, no pill.
    fn close(&mut self) {
        self.canvas = None;
        self.drawn = None;
        self.mode = Mode::Level;
        self.wheel = 0;
        if self.drag.take().is_some() {
            // SAFETY: releases this thread's own capture.
            let _ = unsafe { ReleaseCapture() };
        }
        if let Err(error) = self.window.resize(strip(self.work, self.dpi, self.kind)) {
            eprintln!("Edge slider: hide: {error}");
        }
    }
    fn suppress(&mut self, covered: bool) {
        if covered == self.suppressed {
            return;
        }
        self.suppressed = covered;
        self.hover.reset();
        self.tracking = false;
        self.close();
        if covered {
            self.window.hide();
        } else {
            self.window.show();
        }
    }
    fn set_device(&mut self, name: Option<String>) {
        if name != self.device {
            self.device = name;
            self.name_revision += 1;
        }
    }

    // Input.

    /// Handles one message for this window; true when it was ours.
    fn message(&mut self, message: &MSG, audio: &Audio, now: Instant) -> bool {
        match message.message {
            WM_MOUSEMOVE => {
                self.touched = true;
                self.track();
                self.hover.enter(now);
                let (x, y) = point(message.lParam);
                if self.drag.is_some() {
                    if message.wParam.0 & MK_LBUTTON == 0 {
                        self.end_drag(); // the release was lost
                    } else {
                        self.drag_to(y, audio);
                    }
                } else if let Mode::List { devices, hover, .. } = &mut self.mode {
                    *hover = row_at(self.dpi, devices.len(), x, y);
                }
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                self.hover.leave(now);
            }
            WM_LBUTTONDOWN if self.canvas.is_some() => {
                let (x, y) = point(message.lParam);
                self.press(x, y, audio);
            }
            WM_LBUTTONUP | WM_CAPTURECHANGED => self.end_drag(),
            WM_MOUSEWHEEL if self.canvas.is_some() => {
                let delta = i32::from((message.wParam.0 >> 16) as u16 as i16);
                let (steps, rest) = wheel_steps(self.wheel, delta);
                self.wheel = rest;
                if steps != 0
                    && matches!(self.mode, Mode::Level)
                    && let Some(level) = self.percent(audio)
                {
                    self.set_level(step_level(level, steps), audio);
                }
            }
            edge_uia::SET => self.set_level(message.wParam.0.min(100) as u8, audio),
            _ => return false,
        }
        true
    }
    /// Arms one leave notification; the next move re-arms it.
    fn track(&mut self) {
        if self.tracking {
            return;
        }
        let mut request = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.window.0,
            ..Default::default()
        };
        // SAFETY: requests a leave notification for this owned live HWND.
        self.tracking = unsafe { TrackMouseEvent(&mut request) }.is_ok();
    }
    fn press(&mut self, x: i32, y: i32, audio: &Audio) {
        match &self.mode {
            Mode::List { devices, .. } => {
                if let Some(row) = row_at(self.dpi, devices.len(), x, y) {
                    let device = devices[row].clone();
                    self.choose(device);
                }
            }
            Mode::Level => {
                if self.kind == Kind::Volume && on_icon(x, y, &self.layout(), self.dpi) {
                    self.open_devices(audio);
                } else if self.percent(audio).is_some() {
                    // SAFETY: captures the pointer for this thread's own live window.
                    unsafe { SetCapture(self.window.0) };
                    self.drag_to(y, audio);
                }
            }
        }
    }
    /// Makes `device` the default output and returns to the pill.
    fn choose(&mut self, device: Device) {
        if let Err(error) = policy_config::set_default(&device.id) {
            // The private interface is unavailable on this build: the user changes it in Settings.
            eprintln!("Edge slider: switch device: {error}");
            open_sound_settings();
        }
        self.set_device(Some(device.name));
        self.mode = Mode::Level;
        self.drawn = None;
        if let Err(error) = self.resize_to(self.layout().size) {
            eprintln!("Edge slider: resize: {error}");
            self.close();
        }
    }
    fn open_devices(&mut self, audio: &Audio) {
        let devices = audio.devices();
        if devices.is_empty() {
            return open_sound_settings();
        }
        match self.resize_to(list_layout(self.dpi, devices.len()).0) {
            Ok(_) => {
                self.mode = Mode::List {
                    devices,
                    hover: None,
                };
                self.drawn = None;
            }
            Err(error) => eprintln!("Edge slider: list: {error}"),
        }
    }
    /// Moves the window to `size` at its edge, and its canvas surface with it; returns the surface bounds.
    fn resize_to(&mut self, size: (i32, i32)) -> Result<Bounds> {
        let bounds = place(self.work, self.kind, size);
        self.window.resize(bounds)?;
        let surface = Bounds {
            x: 0,
            y: 0,
            width: bounds.width,
            height: bounds.height,
        };
        if let Some(canvas) = &mut self.canvas {
            canvas.resize(surface)?;
        }
        Ok(surface)
    }
    fn drag_to(&mut self, y: i32, audio: &Audio) {
        let level = level_at(y, &self.layout());
        self.set_level(level, audio);
        // A drag is held until the button goes up; a wheel or UIA change is not.
        if self.canvas.is_some() && self.drag.is_none() {
            self.drag = Some(level);
        }
    }
    fn end_drag(&mut self) {
        if self.drag.take().is_some() {
            // SAFETY: releases this thread's own capture.
            let _ = unsafe { ReleaseCapture() };
        }
    }
    /// Applies a level from the pointer, the wheel or UIA.
    fn set_level(&mut self, percent: u8, audio: &Audio) {
        if self.drag.is_some() {
            self.drag = Some(percent);
        }
        match self.kind {
            Kind::Volume => audio.set(f32::from(percent) / 100.0),
            Kind::Brightness => {
                if self.brightness != Reading::Unsupported {
                    self.brightness = Reading::Level(percent);
                    self.throttle.push(percent);
                }
            }
        }
    }

    // Per pass.

    fn view(&self, audio: &Audio) -> View {
        match &self.mode {
            Mode::List { hover, .. } => View::List { hover: *hover },
            Mode::Level => {
                let unsupported =
                    self.kind == Kind::Brightness && self.brightness == Reading::Unsupported;
                let muted =
                    self.kind == Kind::Volume && audio.level().is_none_or(|level| level.muted);
                let percent = self.percent(audio).unwrap_or(0);
                View::Level {
                    percent,
                    glyph: match self.kind {
                        Kind::Brightness => Glyph::Sun,
                        Kind::Volume if muted => Glyph::Muted,
                        Kind::Volume => Glyph::Speaker,
                    },
                    dim: muted || unsupported,
                    caption: if unsupported {
                        UNSUPPORTED.to_owned()
                    } else if self.drag.is_some() {
                        format!("{percent}%")
                    } else {
                        String::new()
                    },
                    unsupported,
                }
            }
        }
    }
    fn paint(&mut self, audio: &Audio) {
        if self.canvas.is_none() {
            return;
        }
        let view = self.view(audio);
        if self.drawn.as_ref() == Some(&view) {
            return;
        }
        let painted = self
            .canvas
            .as_ref()
            .map(|canvas| match (&view, &self.mode) {
                (
                    View::Level {
                        percent,
                        glyph,
                        dim,
                        caption,
                        unsupported,
                    },
                    _,
                ) => {
                    let layout = self.layout();
                    canvas.paint_slider(&SliderScene {
                        dpi: self.dpi,
                        layout: &layout,
                        percent: *percent,
                        glyph: *glyph,
                        dim: *dim,
                        caption,
                        note: unsupported.then(|| self.kind.right()),
                    })
                }
                (View::List { hover, .. }, Mode::List { devices, .. }) => {
                    let (_, panel) = list_layout(self.dpi, devices.len());
                    let rows: Vec<ListRow> = devices
                        .iter()
                        .enumerate()
                        .map(|(index, device)| ListRow {
                            name: &device.name,
                            current: device.current,
                            rect: row_rect(self.dpi, panel, index),
                        })
                        .collect();
                    canvas.paint_list(&ListScene {
                        dpi: self.dpi,
                        panel,
                        rows: &rows,
                        hover: *hover,
                    })
                }
                _ => Ok(()),
            });
        match painted {
            Some(Err(error)) => {
                eprintln!("Edge slider: paint: {error}");
                self.close(); // the next hover starts over on a fresh canvas
            }
            _ => self.drawn = Some(view),
        }
    }
    /// Shares the level and name with UI Automation when either changes.
    fn publish(&mut self, audio: &Audio) {
        let percent = self.percent(audio).unwrap_or(0);
        let muted = self.kind == Kind::Volume && audio.level().is_some_and(|level| level.muted);
        let enabled = self.enabled(audio);
        let key = (
            percent,
            enabled,
            muted,
            self.name_revision,
            self.brightness == Reading::Pending,
            audio.active(),
        );
        if self.published == Some(key) {
            return;
        }
        self.published = Some(key);
        let name = match (self.kind, enabled) {
            (Kind::Volume, false) => "Volume unavailable, no audio device".to_owned(),
            (Kind::Volume, true) if !audio.active() => "Volume".to_owned(),
            (Kind::Volume, true) => {
                let device = self
                    .device
                    .as_deref()
                    .map(|name| format!(", {name}"))
                    .unwrap_or_default();
                format!(
                    "Volume {percent}%{}{device}",
                    if muted { ", muted" } else { "" }
                )
            }
            (Kind::Brightness, false) => UNSUPPORTED.to_owned(),
            (Kind::Brightness, true) if self.brightness == Reading::Pending => {
                "Brightness".to_owned()
            }
            (Kind::Brightness, true) => format!("Brightness {percent}%"),
        };
        if let Ok(mut state) = self.uia.lock() {
            state.name = name;
            state.percent = percent;
            state.enabled = enabled;
        }
    }
}

/// Every slider of every monitor, and the brightness worker they share.
#[derive(Default)]
pub struct Edges {
    sliders: Vec<Slider>,
    brightness: Option<Brightness>,
}
impl Edges {
    /// Whether Core Audio should run for the volume slider: once the pointer has touched its edge (the
    /// level is read well inside the show delay), or from the start while a UI Automation client listens.
    pub fn wants_audio(&self) -> bool {
        self.sliders
            .iter()
            .any(|slider| slider.kind == Kind::Volume && slider.touched)
            // shortcut: a screen reader gets the volume level without hovering first; any UIA client counts.
            || (self.sliders.iter().any(|slider| slider.kind == Kind::Volume)
                // SAFETY: a parameterless state query.
                && unsafe { UiaClientsAreListening() }.as_bool())
    }
    /// Creates and removes sliders to match the monitors and settings. A monitor the user switched off
    /// gets none; a changed work area or DPI replaces its sliders.
    pub fn reconcile(&mut self, monitors: &[Monitor], settings: &Settings) {
        self.sliders.retain(|slider| {
            slider.kind.enabled(settings)
                && !settings.off_monitors.contains(&slider.monitor)
                && monitors.iter().any(|m| {
                    m.device == slider.monitor && m.work == slider.work && m.dpi == slider.dpi
                })
        });
        for monitor in monitors {
            if settings.off_monitors.contains(&monitor.device) {
                continue;
            }
            for kind in Kind::ALL {
                let exists = self
                    .sliders
                    .iter()
                    .any(|s| s.kind == kind && s.monitor == monitor.device);
                if kind.enabled(settings) && !exists {
                    match Slider::create(kind, monitor) {
                        Ok(slider) => self.sliders.push(slider),
                        Err(error) => {
                            eprintln!("Edge slider {kind:?} on {}: {error}", monitor.device)
                        }
                    }
                }
            }
        }
    }
    /// Drops every pill's canvas (a lost graphics device); the strips stay.
    pub fn release(&mut self) {
        self.sliders.iter_mut().for_each(Slider::close);
    }
    /// Routes a message to the slider owning its window; true when it was handled.
    pub fn message(&mut self, message: &MSG, audio: &Audio, now: Instant) -> bool {
        self.sliders
            .iter_mut()
            .find(|slider| slider.window.0 == message.hwnd)
            .is_some_and(|slider| slider.message(message, audio, now))
    }
    /// One host pass: applies hover deadlines and brightness readings, sends throttled writes, paints what
    /// changed and publishes to UI Automation. `covered` says whether a full-screen app hides a monitor.
    /// Returns the soonest deadline, if any.
    pub fn refresh(
        &mut self,
        now: Instant,
        audio: &Audio,
        graphics: &Graphics,
        palette: Palette,
        covered: impl Fn(&str) -> bool,
    ) -> Option<Instant> {
        while let Some(reading) = self.brightness.as_ref().and_then(Brightness::poll) {
            for slider in &mut self.sliders {
                if slider.kind == Kind::Brightness && slider.target() == reading.target {
                    let was = slider.wide();
                    slider.brightness =
                        reading.percent.map_or(Reading::Unsupported, Reading::Level);
                    if slider.canvas.is_some() && was != slider.wide() {
                        slider.show_pill(graphics, palette); // the note needs the wider window
                    }
                }
            }
        }
        let mut wait: Option<Instant> = None;
        for index in 0..self.sliders.len() {
            let slider = &mut self.sliders[index];
            slider.suppress(covered(&slider.monitor));
            if slider.suppressed {
                continue;
            }
            match slider.hover.tick(now, slider.drag.is_some()) {
                Some(true) => slider.open(graphics, palette, audio),
                Some(false) => slider.close(),
                None => {}
            }
            let target = slider.target();
            let read = std::mem::take(&mut slider.read_wanted);
            let write = slider.throttle.take(now);
            if (read || write.is_some())
                && let Some(worker) = self.worker()
            {
                if read {
                    worker.read(target);
                }
                if let Some(percent) = write {
                    worker.write(target, percent);
                }
            }
            let slider = &mut self.sliders[index];
            slider.paint(audio);
            slider.publish(audio);
            for due in [slider.hover.deadline(), slider.throttle.deadline(now)]
                .into_iter()
                .flatten()
            {
                wait = Some(wait.map_or(due, |soonest| soonest.min(due)));
            }
        }
        wait
    }
    /// The brightness worker, started on first need.
    fn worker(&mut self) -> Option<&Brightness> {
        if self.brightness.is_none() {
            // SAFETY: reads this thread's ID; no pointers.
            self.brightness = Brightness::start(unsafe { GetCurrentThreadId() });
        }
        self.brightness.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Bounds = Bounds {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };

    #[test]
    fn the_strip_is_the_middle_third_of_the_right_or_left_edge() {
        let right = strip(WORK, 96, Kind::Volume);
        assert_eq!((right.x, right.width), (1918, 2));
        assert_eq!((right.y, right.height), (346, 346));
        let left = strip(WORK, 96, Kind::Brightness);
        assert_eq!((left.x, left.width), (0, 2));
        assert_eq!((left.y, left.height), (right.y, right.height));
        // Corners stay free: the strip stays inside the middle third.
        assert!(right.y >= WORK.height / 3 && right.y + right.height <= WORK.height * 2 / 3 + 1);
        // A second monitor to the right, and a work area that starts below a top taskbar, offset the strip.
        let second = Bounds {
            x: 1920,
            y: 40,
            width: 2560,
            height: 1400,
        };
        let strip = strip(second, 144, Kind::Brightness);
        assert_eq!(
            (strip.x, strip.width, strip.y, strip.height),
            (1920, 3, 40 + 466, 466)
        );
        // 200%: the strip scales but never drops below two pixels.
        assert_eq!(super::strip(WORK, 48, Kind::Volume).width, 2);
    }

    #[test]
    fn windows_sit_on_their_edge_centred_and_inside_the_work_area() {
        let size = layout(96, Kind::Volume, false).size;
        assert_eq!(size, (60, 216));
        let right = place(WORK, Kind::Volume, size);
        assert_eq!((right.x + right.width, right.y), (1920, (1040 - 216) / 2));
        assert_eq!(place(WORK, Kind::Brightness, size).x, 0);
        // A window taller or wider than the work area is cut to it.
        let tiny = Bounds {
            x: 100,
            y: 50,
            width: 40,
            height: 100,
        };
        let fitted = place(tiny, Kind::Volume, size);
        assert_eq!(
            (fitted.x, fitted.y, fitted.width, fitted.height),
            (100, 50, 40, 100)
        );
    }

    #[test]
    fn the_pill_layout_scales_and_the_note_goes_beside_it() {
        let normal = layout(96, Kind::Brightness, false);
        assert_eq!(
            normal.pill,
            Bounds {
                x: 8,
                y: 8,
                width: 44,
                height: 200
            }
        );
        // The track sits between the caption and the icon, and the icon inside the pill.
        assert!(normal.track.y > normal.caption.y + normal.caption.height - 1);
        assert!(normal.track.y + normal.track.height < normal.icon.y);
        assert!(normal.icon.y + normal.icon.height <= normal.pill.y + normal.pill.height);
        let big = layout(192, Kind::Brightness, false);
        assert_eq!((big.pill.width, big.pill.height), (88, 400));
        // Wide: the note is on the side away from the screen edge, never over the pill.
        let left = layout(96, Kind::Brightness, true);
        assert_eq!(left.pill.x, 8);
        assert!(left.caption.x >= left.pill.x + left.pill.width);
        let right = layout(96, Kind::Volume, true);
        assert!(right.caption.x + right.caption.width <= right.pill.x);
        assert!(right.pill.x + right.pill.width + 8 == right.size.0);
    }

    #[test]
    fn dragging_maps_the_track_to_zero_through_one_hundred_and_clamps() {
        let l = layout(96, Kind::Volume, false);
        let (top, bottom) = (l.track.y, l.track.y + l.track.height);
        assert_eq!((level_at(top, &l), level_at(bottom, &l)), (100, 0));
        assert_eq!(level_at((top + bottom) / 2, &l), 50);
        // Past either end clamps; a quarter of the way down is 75.
        assert_eq!(
            (level_at(top - 500, &l), level_at(bottom + 500, &l)),
            (100, 0)
        );
        assert_eq!(level_at(top + l.track.height / 4, &l), 75);
        // Monotonic: moving down never raises the level.
        let levels: Vec<u8> = (top - 10..bottom + 10).map(|y| level_at(y, &l)).collect();
        assert!(levels.windows(2).all(|pair| pair[0] >= pair[1]));
        // The icon is a click target, the track is not.
        assert!(on_icon(l.icon.x + 2, l.icon.y + 2, &l, 96));
        assert!(!on_icon(l.track.x, l.track.y, &l, 96));
    }

    #[test]
    fn the_wheel_moves_five_percent_a_notch_and_keeps_the_remainder() {
        assert_eq!(wheel_steps(0, 120), (1, 0));
        assert_eq!(wheel_steps(0, -240), (-2, 0));
        // High-resolution wheels add up to a notch.
        let (steps, rest) = wheel_steps(0, 30);
        assert_eq!((steps, rest), (0, 30));
        let (steps, rest) = wheel_steps(rest, 100);
        assert_eq!((steps, rest), (1, 10));
        assert_eq!(wheel_steps(-90, -60), (-1, -30));
        assert_eq!(
            (
                step_level(50, 1),
                step_level(98, 1),
                step_level(3, -1),
                step_level(0, -9)
            ),
            (55, 100, 0, 0)
        );
    }

    #[test]
    fn the_device_list_picks_the_row_under_the_pointer() {
        let (size, panel) = list_layout(96, 3);
        assert_eq!(size, (266, 3 * 34 + 12 + 16));
        let row = |i| row_rect(96, panel, i);
        assert_eq!(row_at(96, 3, row(1).x + 4, row(1).y + 4), Some(1));
        assert_eq!(
            row_at(96, 3, row(2).x + 4, row(2).y + row(2).height - 1),
            Some(2)
        );
        // The margin, the padding and below the last row are no row.
        assert_eq!(row_at(96, 3, 2, 2), None);
        assert_eq!(row_at(96, 3, row(0).x, row(0).y - 1), None);
        assert_eq!(row_at(96, 3, row(2).x + 4, row(2).y + row(2).height), None);
        assert_eq!(row_at(96, 0, 20, 20), None);
        // Rows never overlap and stay inside the panel.
        assert!(row(0).y + row(0).height == row(1).y);
        assert!(row(2).y + row(2).height <= panel.y + panel.height);
    }

    #[test]
    fn the_pill_shows_after_250_ms_and_hides_800_ms_after_leaving() {
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut hover = Hover::default();
        assert_eq!(
            (hover.deadline(), hover.tick(at(5000), false)),
            (None, None)
        );
        hover.enter(at(0));
        hover.enter(at(100)); // moving inside does not restart the wait
        assert_eq!(hover.deadline(), Some(at(250)));
        assert_eq!(hover.tick(at(249), false), None);
        assert_eq!(hover.tick(at(250), false), Some(true));
        // Shown with the pointer in: nothing is due.
        assert_eq!(
            (hover.deadline(), hover.tick(at(9000), false)),
            (None, None)
        );
        // Leaving arms the hide; returning before it cancels it.
        hover.leave(at(1000));
        assert_eq!(hover.deadline(), Some(at(1800)));
        hover.enter(at(1700));
        assert_eq!(hover.deadline(), None);
        hover.leave(at(2000));
        assert_eq!(hover.tick(at(2799), false), None);
        assert_eq!(hover.tick(at(2800), false), Some(false));
        // Hidden again: the next visit waits the show delay afresh.
        hover.enter(at(3000));
        assert_eq!(hover.deadline(), Some(at(3250)));
        // Leaving before it shows cancels the show.
        hover.leave(at(3100));
        assert_eq!(
            (hover.deadline(), hover.tick(at(9000), false)),
            (None, None)
        );
    }

    #[test]
    fn a_drag_holds_the_pill_open_and_suppression_resets_the_timing() {
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut hover = Hover::default();
        hover.enter(at(0));
        assert_eq!(hover.tick(at(250), false), Some(true));
        hover.leave(at(300));
        // Due while the button is still down: re-armed, not hidden.
        assert_eq!(hover.tick(at(1100), true), None);
        assert_eq!(hover.deadline(), Some(at(1900)));
        assert_eq!(hover.tick(at(1900), false), Some(false));
        // A full-screen app resets everything.
        hover.enter(at(2000));
        hover.reset();
        assert_eq!(
            (hover.deadline(), hover.tick(at(9000), false)),
            (None, None)
        );
    }

    #[test]
    fn brightness_writes_are_limited_to_five_a_second_and_the_latest_wins() {
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut throttle = Throttle::default();
        assert_eq!(
            (throttle.take(at(0)), throttle.deadline(at(0))),
            (None, None)
        );
        // The first value goes out at once.
        throttle.push(10);
        assert_eq!(throttle.deadline(at(0)), Some(at(0)));
        assert_eq!(throttle.take(at(0)), Some(10));
        // A burst inside the gap sends nothing and keeps only the newest value.
        for (ms, value) in [(10, 20), (50, 30), (120, 40), (199, 50)] {
            throttle.push(value);
            assert_eq!(throttle.take(at(ms)), None);
        }
        assert_eq!(throttle.deadline(at(199)), Some(at(200)));
        assert_eq!(throttle.take(at(200)), Some(50));
        // Nothing waiting, nothing sent, and the gap counts from the last send.
        assert_eq!(throttle.take(at(900)), None);
        throttle.push(60);
        assert_eq!(throttle.take(at(900)), Some(60));
        // Five sends fit in a second of constant movement.
        let mut sent = 0;
        let mut throttle = Throttle::default();
        for ms in (0..1000).step_by(10) {
            throttle.push((ms / 10) as u8);
            sent += usize::from(throttle.take(at(ms)).is_some());
        }
        assert_eq!(sent, 5);
    }

    #[test]
    fn sliders_follow_the_settings_and_each_kind_has_its_own_edge() {
        let mut settings = Settings::default();
        assert!(Kind::Volume.enabled(&settings) && Kind::Brightness.enabled(&settings));
        settings.edge_volume = false;
        assert!(!Kind::Volume.enabled(&settings) && Kind::Brightness.enabled(&settings));
        assert!(Kind::Volume.right() && !Kind::Brightness.right());
    }
}
