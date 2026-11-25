mod command_center;
mod dock_widgets;
mod interactions;
mod levels;
mod menus;
mod notch_clicks;
mod notices;
mod settings_bridge;
mod shots;
use crate::{
    agent,
    audio::{self, Audio},
    catalog_timing::milliseconds_until,
    clipboard::{self, Clips},
    clipboard_os, clock,
    dock::{self, Button, Buttons, DockItem, Hit},
    dock_menu::{self, Event, Item},
    dock_tuck, edge,
    eyes::{self, Eyes},
    graphics::{
        COVER_DIPS, Canvas, DockScene, DockSlot, Fx, Graphics, Grip, NotchScene, RailScene,
        ShelfSlot,
    },
    hotkeys,
    icons::{self, IconPixels, IconService},
    ipc::{Command, Controls},
    keys,
    layout::{self, Bounds, DockStyle, SurfaceKind, placement_styled},
    limits::{self, Limits},
    limits_os,
    media::{self, Media, Progress, Track},
    motion::{self, Tween},
    native_window::{self, NativeWindow, WindowClass},
    pins::Pins,
    pomo,
    power::{self, Power},
    prayer_os, prayer_tab, preview, privacy, quick, remind,
    remind_box::RemindBox,
    settings::{self, Settings},
    settings_window::{SettingsWindow, SoundAction},
    shelf::{self, Check, Shelf},
    shelf_os::{self, DropEvent, Drops},
    sort,
    stopwatch::Stopwatch,
    store::{self, Flush, Pending, Store},
    tabs::{Tab, Tabs},
    text_settings::TextSettings,
    theme::Palette,
    timer::{self, Timers},
    toast,
    topology::{self, Monitor},
    water,
    weather::{self, Feed},
    weather_os,
    window_catalog::{self, Catalog},
    window_model::{Identity, WindowKey, WindowModel},
};
use std::{
    cell::Cell,
    mem::size_of,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{E_FAIL, HANDLE, HWND, LPARAM, WAIT_EVENT, WAIT_FAILED, WAIT_OBJECT_0},
    System::Diagnostics::Debug::MessageBeep,
    System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
    System::Ole::{OleInitialize, OleUninitialize},
    System::Threading::INFINITE,
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{
            GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
            VK_CONTROL, VK_MENU,
        },
        WindowsAndMessaging::*,
    },
};
use windows::core::{Error, Result, s, w};

#[derive(PartialEq, Eq)]
pub enum Probe {
    None,
    #[cfg(debug_assertions)]
    Initialization,
    #[cfg(debug_assertions)]
    Callback,
    /// Debug builds only: one synthetic device-removed failure right after startup.
    #[cfg(debug_assertions)]
    DeviceLoss,
}

struct Apartment;
impl Apartment {
    fn enter() -> Result<Self> {
        // SAFETY: main thread owns all COM objects and drops them before Apartment. OLE (an STA, as
        // COM was) is what `RegisterDragDrop` for the shelf needs.
        unsafe {
            OleInitialize(None)?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: balances successful initialization on this same thread.
        unsafe {
            OleUninitialize();
        }
    }
}

struct NotchShadow {
    painted: Cell<Option<(f32, bool, f32)>>,
    canvas: Canvas,
    window: NativeWindow,
}
impl NotchShadow {
    fn create(graphics: &Graphics, bounds: Bounds, palette: Palette) -> Result<Self> {
        let window = NativeWindow::create_shadow(bounds)?;
        let canvas = Canvas::create(graphics, window.0, bounds, palette)?;
        Ok(Self {
            canvas,
            window,
            painted: Cell::new(None),
        })
    }
    fn paint(&self, scene: &NotchScene, notch: HWND) -> Result<()> {
        let notice = if scene.extra.footer() {
            scene.fx.notice
        } else {
            0.0
        };
        let frame = if motion::smooth(scene.progress) == 0.0 && notice == 0.0 {
            (0.0, false, 0.0)
        } else {
            (scene.progress, scene.track.is_some(), notice)
        };
        if self.painted.get() == Some(frame) {
            return Ok(());
        }
        // Follow the body even when a test moves its HWND; keep the shadow immediately behind it.
        // SAFETY: both HWNDs are owned on this UI thread; no activation or focus change.
        unsafe {
            let mut bounds = Default::default();
            GetWindowRect(notch, &mut bounds)?;
            SetWindowPos(
                self.window.0,
                Some(notch),
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                SWP_NOACTIVATE,
            )?;
        }
        self.canvas.paint_notch_shadow(scene)?;
        self.painted.set(Some(frame));
        Ok(())
    }
}

/// What one pass must repaint on a notch. Requests only grow within a pass, so a notch is painted
/// once, by the strongest: a full paint covers a bars-only one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Repaint {
    #[default]
    Nothing,
    /// Only the visualizer slot, for a settled open panel whose bar heights changed.
    Meter,
    Full,
}

/// What the notches show besides the media session (features tabs-eyes): the tabs, and the mascot.
struct Pages {
    tabs: Tabs,
    /// `None` with "Mascot eyes" off: no state, no deadline and no drawing.
    eyes: Option<Eyes>,
    /// The consent-store watch (features B5): `None` with "Privacy indicator" off, which closes its handle.
    watch: Option<privacy::Watch>,
    /// The mic and camera dots: what is in use and their fade.
    dots: privacy::Dots,
    /// The last pointer input on a notch, and the local hour at the last clock tick: what the
    /// mascot's sleepy mood reads. Then that tick's Hijri date, empty with the Prayer tab off.
    last_input: Instant,
    hour: u8,
    hijri: String,
    /// The Weather page's text, rebuilt by `refresh_weather` only while that page shows, and the feed
    /// revision and minute it was built for.
    weather: weather::View,
    weather_key: Option<(u32, String)>,
    /// The Prayer page's text, rebuilt by `refresh_prayer` only while that page shows, and what it was built for.
    prayer: prayer_tab::View,
    prayer_key: Option<(String, u32, crate::prayer::Method, [i8; 6])>,
    /// The Limits page, rebuilt by `refresh_limits` only while that page shows, and what it was built for.
    limits: limits::View,
    limits_key: Option<(u32, bool, String)>,
    moon: prayer_tab::MoonDay,
    pill_low: bool,
    /// The month selected in the Calendar popup, or today's month.
    month: Option<(u16, u16)>,
    /// The System widget's last readings.
    system: crate::widgets::SystemSample,
    system_opened: Option<Instant>,
    widget_readings: crate::graphics::Readings,
    /// The Pomodoro's phase rules; its running phase is a tagged timer in `Desktop::timers`.
    pomo: pomo::Pomo,
    /// A paused plain countdown: seconds left and its full length (not stored, like a paused Pomodoro).
    held: Option<(u64, u64)>,
    /// The Timer page's stopwatch: shown when no countdown is.
    stopwatch: Stopwatch,
    /// Reminders waiting: the idle Timer page lists them.
    reminders: remind::Reminders,
}
impl Pages {
    fn new(settings: &Settings, last: Tab) -> Self {
        let mut pages = Self {
            tabs: Tabs::new(settings.tabs, last),
            eyes: settings.eyes.then(|| {
                // Start-up is a wake: the eyes open once, then give way to the time.
                let mut eyes = Eyes::default();
                eyes.wake(Instant::now());
                eyes
            }),
            watch: None,
            dots: privacy::Dots::default(),
            last_input: Instant::now(),
            hour: 12,
            hijri: String::new(),
            weather: weather::View::default(),
            weather_key: None,
            prayer: prayer_tab::View::default(),
            prayer_key: None,
            limits: limits::View::default(),
            limits_key: None,
            moon: prayer_tab::moon_day(prayer_os::local_now().ymd, false, &prayer_os::hijri),
            pill_low: settings.widgets.pill_low,
            month: None,
            system: Default::default(),
            system_opened: None,
            widget_readings: Default::default(),
            pomo: pomo::Pomo::default(),
            held: None,
            stopwatch: Stopwatch::default(),
            reminders: remind::Reminders::default(),
        };
        pages.set_privacy(settings.privacy, Instant::now());
        pages
    }
    /// The idle Timer page's waiting reminders as (clock time, words), the soonest first, at most what it has rows for.
    fn reminder_rows(&self) -> Vec<(String, String)> {
        let (wall, day) = (timer::wall_now(), i64::from(prayer_os::local_now().secs));
        self.reminders
            .waiting()
            .iter()
            .take(layout::REMIND_ROWS)
            .map(|(expiry, label)| {
                let secs = day + *expiry as i64 - wall as i64;
                (prayer_os::time_label(secs as i32), remind::shown(label))
            })
            .collect()
    }
    /// The Timer page's content: the soonest countdown, named for its Pomodoro phase, or a paused phase
    /// when nothing runs.
    fn timer_view(&self, timers: &Timers, now: Instant) -> Option<timer::View> {
        let view = timers.view(now);
        if let Some(left) = self.pomo.left
            && view.as_ref().is_none_or(|view| view.done)
        {
            return Some(timer::View::paused(
                left,
                self.pomo.total,
                self.pomo.title(true),
                Some(self.pomo.cycle()),
            ));
        }
        if let Some((left, total)) = self.held
            && view.as_ref().is_none_or(|view| view.done)
        {
            return Some(timer::View::paused(left, total, String::new(), None));
        }
        view.map(|mut view| {
            if view.tagged {
                view.label = self.pomo.title(false);
                view.total = self.pomo.total;
                view.cycle = Some(self.pomo.cycle());
            }
            view
        })
        .or_else(|| self.stopwatch.view(now))
    }
    /// Starts or stops the registry watch with the setting and reads the devices once; true when the dots changed.
    fn set_privacy(&mut self, on: bool, now: Instant) -> bool {
        self.watch = on.then(privacy::Watch::new).flatten();
        let used = self.watch.as_ref().map(privacy::Watch::read);
        self.dots.set(used.unwrap_or_default(), now)
    }
    /// The track the notch shows: none with the Media tab off, so a session costs nothing then.
    fn track<'a>(&self, media: &'a Media) -> Option<&'a Track> {
        self.tabs
            .on(Tab::Media)
            .then(|| media.view().shown())
            .flatten()
    }
    /// The page the open panel shows. With every tab off the panel is the plain clock (the Media page).
    fn page(&self, media_live: bool) -> Tab {
        self.tabs.active(media_live).unwrap_or(Tab::Media)
    }
    /// The handle's layout; `None` with fewer than two tabs.
    fn rail(&self, media_live: bool) -> Option<layout::Rail> {
        let visible = self.tabs.visible();
        let active = visible.position(self.page(media_live))?;
        visible.handle().then_some(layout::Rail {
            count: visible.as_slice().len(),
            active,
            expansion: [0.0; 7],
        })
    }
    fn touch(&mut self) {
        self.last_input = Instant::now();
    }
}

/// Dot expansion and the sliding indicator share the existing frame clock. Reduced motion lands instantly.
#[derive(Clone, Copy)]
struct RailFx {
    ind: Tween,
    pop: Tween,
    expansion: [Tween; 7],
    press: Tween,
    /// The wash behind the Media page's glance line while the pointer is on it (`tabs::glance`).
    line: Tween,
    /// The selected tab: it stays open (glyph and name) so the rail reads as tabs, not as dots.
    active: usize,
}

impl RailFx {
    fn new() -> Self {
        Self {
            ind: Tween::rest(0.0),
            pop: Tween::rest(1.0),
            expansion: std::array::from_fn(|index| Tween::rest(f32::from(index == 0))),
            press: Tween::rest(0.0),
            line: Tween::rest(0.0),
            active: 0,
        }
    }
    fn running(&self, now: Instant) -> bool {
        [self.ind, self.pop, self.press, self.line]
            .iter()
            .chain(self.expansion.iter())
            .any(|tween| tween.running(now))
    }
    /// Opens the hovered dot and the selected one; the others settle, including interrupted transitions.
    fn aim(&mut self, hover: Option<usize>) {
        for (index, tween) in self.expansion.iter_mut().enumerate() {
            let hovered = hover == Some(index);
            Self::go(
                tween,
                if hovered || index == self.active {
                    1.0
                } else {
                    0.0
                },
                if hovered {
                    motion::POP_TIME
                } else {
                    motion::HOVER_IN_TIME
                },
                if hovered { motion::POP } else { motion::SETTLE },
            );
        }
    }
    fn go(tween: &mut Tween, to: f32, time: Duration, curve: motion::Curve) {
        tween.retarget(
            to,
            if motion::calm() { Duration::ZERO } else { time },
            curve,
        );
    }
}

struct Surface {
    role: Role,
    canvas: Canvas,
    window: NativeWindow,
    /// Owning monitor's device name; surfaces are keyed by it plus their kind.
    monitor: String,
    /// Monitor DPI; the notch's also includes the Windows text-size factor.
    dpi: u32,
    bounds: Bounds,
    hovered: bool,
    /// False while a full-screen app covers this monitor; hidden surfaces keep their state.
    visible: bool,
    /// Frames are painted only while set; `animate` clears it when motion settles.
    live: bool,
    /// Notch: 0 collapsed to 1 expanded. Dock: pointer-inside progress of the bar fill.
    motion: Tween,
    /// A dock's slide in from below its bottom edge: 0 hidden, 1 settled. Unused by the notch.
    slide: Tween,
    /// A dock sliding down before it hides: it stays visible until `animate` sees the slide end.
    leaving: bool,
    /// Notch: the progress row last drawn, if any.
    played: Option<Progress>,
    /// Notch: the visualizer bar heights last drawn.
    bars: [i32; audio::BARS],
    /// Notch: what this pass has asked to repaint (`Desktop::paint_requested`).
    repaint: Repaint,
    /// Notch: the tab cell under the pointer (a position in the visible tabs), for its hover wash.
    tab_hover: Option<usize>,
    rail_fx: RailFx,
    /// Notch: the pointer on the volume bar (hover, and a captured drag).
    grip: Grip,
    /// Notch: the volume bar's drawn fill (percent), easing toward the real level; a drag sets it at once.
    volume_fill: Tween,
    /// Notch: the page last drawn (its tab index), the slide and fade of a switch to the current one (1 at
    /// rest) and its direction, and the notice's entry with whether one showed.
    page_at: usize,
    /// The page a switch left, while its slide runs, and the last notice text (it fades out after it ends).
    page_from: Option<Tab>,
    last_notice: String,
    last_notice_extra: power::Extra,
    notice_input: notch_clicks::NoticeInput,
    page_dir: f32,
    page_fx: Tween,
    notice_fx: Tween,
    notice_on: bool,
    /// Dock: a hover change has not been painted yet (`paint_dock` starts its clocks).
    hover_unpainted: bool,
    /// Dock: floating or attached to the bottom edge, and which taskbar buttons it shows.
    dock_style: DockStyle,
    dock_buttons: Buttons,
    /// Notch: shown over a full-screen app for a prayer alert only; hidden again once it has retracted.
    overlay: bool,
}
impl Surface {
    /// Ends a volume-bar drag: the thumb shrinks and the capture goes.
    fn end_grip(&mut self) {
        if std::mem::take(&mut self.grip.drag) {
            // SAFETY: releases this thread's own capture; harmless if it is already gone.
            let _ = unsafe { ReleaseCapture() };
            self.request(Repaint::Full);
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        shelf_os::revoke(self.window.0);
    }
}
/// State only one surface kind needs.
#[allow(clippy::large_enum_variant)] // one value per surface, never moved in bulk
enum Role {
    /// Pointer inside the hot band (not its shadow), the instant hover intent opens the notch, and the
    /// window region last applied (it follows the drawn shape frame by frame).
    Notch {
        shadow: NotchShadow,
        engaged: bool,
        intent: Option<Instant>,
        /// When an open notch the pointer left closes (`LINGER_TIME`); re-entry cancels it.
        linger: interactions::Linger,
        area: Vec<Bounds>,
    },
    /// Hovered cell and per-cell hover progress.
    Dock {
        hover: Option<usize>,
        slots: Vec<Tween>,
    },
}
impl Surface {
    #[allow(clippy::too_many_arguments)]
    fn create(
        graphics: &Graphics,
        kind: SurfaceKind,
        monitor: &Monitor,
        dpi: u32,
        slots: usize,
        drops: &Drops,
        palette: Palette,
        dock_style: DockStyle,
        dock_buttons: Buttons,
    ) -> Result<Self> {
        let bounds = placement_styled(
            monitor.dock_room(dock_style),
            dpi,
            kind,
            slots,
            dock_style,
            dock_buttons,
        );
        let shadow = if kind == SurfaceKind::Notch {
            Some(NotchShadow::create(graphics, bounds, palette)?)
        } else {
            None
        };
        let window = NativeWindow::create(bounds, kind)?;
        if let Err(error) = shelf_os::register(window.0, drops, kind == SurfaceKind::Dock) {
            eprintln!("File drops unavailable: {error}");
        }
        let role = match shadow {
            None => Role::Dock {
                hover: None,
                slots: vec![Tween::rest(0.0); dock::cell_count(dock_buttons, slots)],
            },
            // The first paint, which always precedes showing the window, applies the input region
            // of the plain or media pill.
            Some(shadow) => Role::Notch {
                shadow,
                engaged: false,
                intent: None,
                linger: Default::default(),
                area: Vec::new(),
            },
        };
        let canvas = Canvas::create(graphics, window.0, bounds, palette)?;
        // Created hidden; the owner shows it after the first paint.
        Ok(Self {
            role,
            canvas,
            window,
            monitor: monitor.device.clone(),
            dpi,
            bounds,
            hovered: false,
            visible: false,
            live: false,
            motion: Tween::rest(0.0),
            slide: Tween::rest(0.0),
            leaving: false,
            played: None,
            bars: [0; audio::BARS],
            repaint: Repaint::Nothing,
            tab_hover: None,
            rail_fx: RailFx::new(),
            grip: Grip::default(),
            volume_fill: Tween::rest(0.0),
            page_at: 0,
            page_from: None,
            last_notice: String::new(),
            last_notice_extra: power::Extra::None,
            notice_input: notch_clicks::NoticeInput::default(),
            page_dir: 1.0,
            page_fx: Tween::rest(1.0),
            notice_fx: Tween::rest(0.0),
            notice_on: false,
            hover_unpainted: false,
            overlay: false,
            dock_style,
            dock_buttons,
        })
    }
    /// Closed and not moving: nothing of the panel is drawn.
    fn collapsed(&self, now: Instant) -> bool {
        self.motion.target() <= 0.0 && !self.motion.running(now)
    }
    fn request(&mut self, repaint: Repaint) {
        self.repaint = self.repaint.max(repaint);
    }
    /// A visible notch that is open or opening; only these read and draw playback progress.
    fn expanded(&self) -> bool {
        self.visible && self.kind() == SurfaceKind::Notch && self.motion.target() >= 1.0
    }
    fn kind(&self) -> SurfaceKind {
        match self.role {
            Role::Notch { .. } => SurfaceKind::Notch,
            Role::Dock { .. } => SurfaceKind::Dock,
        }
    }
    /// Hides or restores the surface (and the notch's shadow) without releasing anything. Hidden
    /// surfaces are not painted, so callers paint before restoring one.
    fn set_visible(&mut self, visible: bool) {
        if matches!(self.role, Role::Dock { .. }) && self.visible {
            // A visible dock slides down before its window hides (`animate` finishes it); showing again
            // while it leaves just turns the slide around.
            if !visible && !self.leaving {
                (self.leaving, self.live) = (true, true);
                self.slide.retarget(0.0, dock::SLIDE_TIME, motion::SETTLE);
            } else if visible && self.leaving {
                (self.leaving, self.live) = (false, true);
                self.slide.retarget(1.0, dock::SLIDE_TIME, motion::SETTLE);
            }
            if self.leaving || visible {
                return;
            }
        }
        self.apply_visible(visible);
    }
    fn apply_visible(&mut self, visible: bool) {
        self.leaving = false;
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        let shadow = match &self.role {
            Role::Notch { shadow, .. } => Some(&shadow.window),
            Role::Dock { .. } => None,
        };
        for window in shadow.into_iter().chain([&self.window]) {
            if visible {
                window.show()
            } else {
                window.hide()
            }
        }
        if visible && matches!(self.role, Role::Dock { .. }) {
            // The dock was painted below its window (slide 0): it rises over `SLIDE_TIME`.
            self.slide.retarget(1.0, dock::SLIDE_TIME, motion::SETTLE);
            self.live = true;
        }
        if !visible {
            self.notice_input.hide();
            // A hidden window gets no leave notification and needs no frames: settle at rest now.
            (self.hovered, self.live, self.motion) = (false, false, Tween::rest(0.0));
            self.slide = Tween::rest(0.0);
            self.tab_hover = None;
            let active = self.rail_fx.active;
            self.rail_fx.expansion =
                std::array::from_fn(|index| Tween::rest(f32::from(index == active)));
            self.rail_fx.press = Tween::rest(0.0);
            match &mut self.role {
                Role::Notch {
                    engaged,
                    intent,
                    linger,
                    ..
                } => {
                    (*engaged, *intent) = (false, None);
                    *linger = Default::default();
                }
                Role::Dock { hover, slots, .. } => {
                    *hover = None;
                    slots.fill(Tween::rest(0.0));
                }
            }
        }
    }
    /// Ends a timer completion's expansion: closes a notch nobody is pointing at. A pointer keeps it open,
    /// and the usual leave closes it.
    fn close_alert(&mut self) {
        if self.expanded() && matches!(self.role, Role::Notch { engaged: false, .. }) {
            self.motion.retarget(0.0, motion::CLOSE_TIME, motion::CLOSE);
            self.live = true;
        }
    }
    /// The pointer is on a rail dot (or left it): that dot opens.
    fn set_tab_hover(&mut self, tab: Option<usize>) {
        if tab == self.tab_hover {
            return;
        }
        self.tab_hover = tab;
        RailFx::go(&mut self.rail_fx.press, 0.0, motion::POP_TIME, motion::POP);
        self.rail_fx.aim(self.tab_hover);
        self.live = true;
    }
    fn set_line_hover(&mut self, over: bool) {
        if over != (self.rail_fx.line.target() > 0.0) {
            let (to, time) = if over {
                (1.0, motion::HOVER_IN_TIME)
            } else {
                (0.0, motion::HOVER_OUT_TIME)
            };
            RailFx::go(&mut self.rail_fx.line, to, time, motion::SETTLE);
            self.live = true;
        }
    }
    fn tab_rail(&self, pages: &Pages, media: bool, now: Instant) -> Option<layout::Rail> {
        pages.rail(media).map(|mut rail| {
            rail.expansion = self.rail_fx.expansion.map(|tween| tween.value(now));
            rail
        })
    }
    fn leave(&mut self) {
        if self.hovered {
            self.hovered = false;
            if self.kind() == SurfaceKind::Dock {
                self.hover_dock(None, false);
            } else {
                self.set_notch_hover(false);
                self.set_tab_hover(None);
                self.set_line_hover(false);
            }
        }
    }
    /// Starts leave tracking on first entry; returns true only for that transition.
    fn track_pointer(&mut self) -> Result<bool> {
        if self.hovered {
            return Ok(false);
        }
        let mut tracking = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.window.0,
            ..Default::default()
        };
        // SAFETY: tracking only requests leave notification for this owned live HWND.
        unsafe {
            TrackMouseEvent(&mut tracking)?;
        }
        self.hovered = true;
        Ok(true)
    }
    /// Draws one frame.
    // shortcut: eight arguments, each a distinct borrowed part of `Desktop`; bundle them into a view
    // struct if a ninth is needed.
    #[allow(clippy::too_many_arguments)]
    fn paint_notch(
        &mut self,
        (time, date): (&str, &str),
        media: &Media,
        audio: &Audio,
        timers: &Timers,
        power: &Power,
        shelf: &ShelfView,
        pages: &Pages,
    ) -> Result<()> {
        let track = pages.track(media);
        let page = pages.page(track.is_some());
        let now = Instant::now();
        // A switched-off Timer tab draws and ticks nothing.
        let timer = pages
            .tabs
            .on(Tab::Timer)
            .then(|| pages.timer_view(timers, now))
            .flatten();
        // The idle Timer page lists the reminders waiting and grows to hold them.
        let reminders = if page == Tab::Timer && timer.as_ref().is_none_or(|view| view.done) {
            pages.reminder_rows()
        } else {
            Vec::new()
        };
        layout::set_page_height(layout::reminders_height(reminders.len()));
        let progress = self.motion.value(now);
        let notice = power.text();
        let extra = power.extra();
        // The screenshot pill holds the row while no notice shows; its row eases in and out like one.
        let shot_on = shelf.shots.showing() && notice.is_none();
        if shot_on {
            self.last_notice.clear();
        }
        // A switch of page while the panel is open slides and fades the new one in; a notice eases in.
        if let Some(dir) = motion::page_switch(self.page_at, page.index(), progress > 0.9)
            && self.visible
        {
            self.page_dir = dir;
            self.page_from = Tab::ALL.get(self.page_at).copied();
            self.page_fx = Tween::rest(0.0);
            self.page_fx.retarget(1.0, motion::PAGE_TIME, motion::SLIDE);
            self.live = true;
        }
        self.page_at = page.index();
        if let Some(text) = notice
            && self.last_notice != text
        {
            self.last_notice = text.to_owned();
        }
        let prior_extra = self.last_notice_extra;
        if notice.is_some() || shot_on {
            self.last_notice_extra = extra;
        }
        if (notice.is_some() || shot_on) != self.notice_on {
            self.notice_on = !self.notice_on;
            if self.visible {
                // Fades in, and out again when it ends (the text stays in `last_notice` for that).
                let (to, from) = if self.notice_on {
                    (1.0, 0.0)
                } else {
                    (0.0, 1.0)
                };
                if !prior_extra.footer() && !extra.footer() {
                    self.notice_fx = Tween::rest(from);
                }
                self.notice_fx
                    .retarget(to, motion::PAGE_TIME, motion::SLIDE);
                self.live = true;
            } else {
                self.notice_fx = Tween::rest(if self.notice_on { 1.0 } else { 0.0 });
            }
        }
        if !self.page_fx.running(now) {
            self.page_from = None;
        }
        let fading_out = notice.is_none() && self.notice_fx.running(now);
        let notice = notice.or((shot_on || fading_out).then_some(self.last_notice.as_str()));
        let extra = if fading_out && self.last_notice_extra.footer() {
            self.last_notice_extra
        } else {
            extra
        };
        let shown = self.notice_fx.value(now);
        layout::set_notice_footer(page, track.is_some(), notice.is_some() && extra.footer());
        // The plain panel makes room for the notice's row as it eases in, and gives it back as it leaves.
        if power.text().is_some() {
            layout::set_notice_ask(matches!(power.extra(), power::Extra::Ask(_)));
            layout::set_notice_shot(false);
        } else if shot_on {
            layout::set_notice_ask(false);
            layout::set_notice_shot(true);
        }
        layout::set_notice_shown(if notice.is_some() { shown } else { 0.0 });
        let footer = layout::NoticeFooter::current(
            self.bounds.width,
            self.dpi,
            progress,
            track.is_some(),
            shown,
        );
        let level = audio.level().map_or(0.0, |level| f32::from(level.percent));
        if self.grip.drag {
            self.volume_fill = Tween::rest(level); // the thumb follows the pointer without lag
        } else {
            self.volume_fill
                .retarget(level, VOLUME_EASE, motion::SETTLE);
        }
        let volume_fill = self.volume_fill.value(now);
        // The panel's row needs the bar's width; the collapsed pill shows only the elapsed time, so a
        // 1 px bar keeps its repaints to whole seconds.
        let width = if motion::reveal(progress) > 0.0 {
            layout::media_progress(self.dpi).width
        } else {
            1
        };
        self.played = track.and_then(|track| track.progress(media::now_ticks(), width));
        // Paused bars lie flat at once; the meter stops on the next pass.
        let peaks = match track {
            Some(track) if track.playing => audio.peaks(),
            _ => [0.0; audio::BARS],
        };
        self.bars = layout::meter_heights(peaks, self.dpi);
        let rail = self.tab_rail(pages, track.is_some(), now);
        let Role::Notch { shadow, area, .. } = &mut self.role else {
            return Ok(());
        };
        let visible = pages.tabs.visible();
        // The indicator slides to the new tab with a squash that pops on arrival; a closed panel or a
        // changed tab set puts it in place without a frame.
        if let Some(rail) = rail {
            let at = rail.active as f32;
            if self.rail_fx.active != rail.active {
                self.rail_fx.active = rail.active;
                self.rail_fx.aim(self.tab_hover);
                self.live = true;
            }
            let fx = &mut self.rail_fx;
            if fx.ind.target() != at {
                if self.visible && progress > 0.9 && !motion::calm() {
                    RailFx::go(&mut fx.ind, at, motion::PAGE_TIME, motion::SLIDE);
                    fx.pop = Tween::rest(0.0);
                    RailFx::go(&mut fx.pop, 1.0, motion::POP_TIME, motion::POP);
                    self.live = true;
                } else {
                    fx.ind = Tween::rest(at);
                }
            }
        }
        // Input and drawing use the same frame progress and shape, without a speculative growth lead.
        let rows = layout::notch_rows(self.bounds, self.dpi, progress, track.is_some(), rail);
        if *area != rows {
            self.window.set_notch_region(&rows)?;
            *area = rows;
        }
        let shelf_on = pages.tabs.on(Tab::Shelf);
        let copied = shelf_on && shelf.copied;
        let previews: Vec<String> = if copied {
            shelf
                .clips
                .visible()
                .iter()
                .map(|t| clipboard::preview(t))
                .collect()
        } else {
            Vec::new()
        };
        let mut slots: Vec<ShelfSlot> = if copied {
            let clip = |name, more| ShelfSlot {
                name,
                missing: false,
                more,
                add: false,
                thumb: None,
            };
            let more = shelf.clips.has_more().then(|| clip("", true));
            previews
                .iter()
                .map(|text| clip(text, false))
                .chain(more)
                .collect()
        } else {
            shelf
                .shelf
                .visible()
                .iter()
                .filter(|_| shelf_on)
                .map(|entry| ShelfSlot {
                    name: &entry.name,
                    missing: entry.check == Check::Missing,
                    more: false,
                    add: false,
                    thumb: entry.shot.as_ref().map(|kept| &kept.0.thumb),
                })
                .collect()
        };
        if shelf_on && !copied && (shelf.shelf.overflow() || shelf.shelf.is_empty()) {
            slots.push(ShelfSlot {
                name: "",
                missing: false,
                more: shelf.shelf.overflow(),
                add: shelf.shelf.is_empty(),
                thumb: None,
            });
        }
        let scene = NotchScene {
            time,
            date,
            hijri: &pages.hijri,
            track,
            art: media.art(),
            progress,
            dpi: self.dpi,
            played: self.played,
            volume: audio.level(),
            bars: self.bars,
            timer: timer.as_ref(),
            reminders: &reminders,
            notice,
            extra,
            footer,
            shot: shots::scene(&shelf.shots, shot_on, now),
            shelf: &slots,
            shelf_undo: shelf.removed.is_some() && !copied,
            shelf_hover: self.hovered,
            clips: copied,
            tab: page,
            low_battery: pages.pill_low && power::Raw::read().is_some_and(power::Raw::low),
            rail: rail.map(|layout| RailScene {
                layout,
                tabs: visible.as_slice(),
                ind: self.rail_fx.ind.value(now),
                pop: self.rail_fx.pop.value(now),
                press: self.rail_fx.press.value(now),
            }),
            eyes: pages.eyes.map(|eyes| eyes.scene()),
            privacy: pages.dots.alpha(),
            grip: self.grip,
            line_wash: self.rail_fx.line.value(now),
            volume_fill,
            weather: [Some(page), self.page_from]
                .iter()
                .any(|p| matches!(p, Some(Tab::Weather)))
                .then_some(&pages.weather),
            prayer: [Some(page), self.page_from]
                .iter()
                .any(|p| matches!(p, Some(Tab::Prayer)))
                .then_some(&pages.prayer),
            leaving: self.page_from,
            fx: Fx {
                page: self.page_fx.value(now),
                dir: self.page_dir,
                notice: self.notice_fx.value(now),
            },
        };
        shadow.paint(&scene, self.window.0)?;
        let glance =
            crate::tabs::glance(&pages.tabs, timer.as_ref(), &pages.prayer, &pages.weather);
        let status = track.is_none().then(|| media.view().status()).flatten();
        let status = status.or(glance.as_ref().map(|(text, _)| text.as_str()));
        self.canvas
            .paint_notch(&scene, status.filter(|_| pages.tabs.on(Tab::Media)))?;
        let presentation = (extra.footer() && notice.is_some() && shown > 0.0)
            .then(|| motion::reveal(progress) == 0.0);
        self.notice_input
            .painted(presentation, footer, extra, power.started());
        Ok(())
    }
    /// Repaints only the visualizer slot with the new bar heights.
    fn paint_meter(&mut self, peaks: [f32; audio::BARS]) -> Result<()> {
        self.bars = layout::meter_heights(peaks, self.dpi);
        self.canvas.paint_meter(self.dpi, self.bars)
    }
    /// Pointer entered or left the notch. Entering swells the pill at once and opens it only after
    /// `INTENT_TIME` (a pointer passing along the screen edge never pops the panel open); leaving
    /// closes from wherever the motion is. The HWND itself never moves.
    fn set_notch_hover(&mut self, over: bool) {
        let Role::Notch {
            engaged,
            intent,
            linger,
            ..
        } = &mut self.role
        else {
            return;
        };
        if over == *engaged {
            return;
        }
        *engaged = over;
        linger.clear();
        if !over {
            if intent.take().is_none() && self.motion.target() >= 1.0 {
                // Open: a slip past the edge gets a short grace (`Desktop::refresh_close`), no frames meanwhile.
                linger.leave(Instant::now());
                return;
            }
            self.motion.retarget(0.0, motion::CLOSE_TIME, motion::CLOSE);
        } else if motion::click_to_open() {
            // Click mode: the pill only swells (and never shrinks an open panel); a click opens it.
            if swells(self.motion.target()) {
                self.motion
                    .retarget(NOTCH_SWELL, motion::INTENT_TIME, motion::SETTLE);
            }
        } else if self.motion.value(Instant::now()) > NOTCH_SWELL {
            // Re-entered while still closing: reopen directly.
            open_notch(intent, &mut self.motion);
        } else {
            *intent = Some(Instant::now() + motion::time(motion::INTENT_TIME));
            self.motion
                .retarget(NOTCH_SWELL, motion::INTENT_TIME, motion::SETTLE);
        }
        self.live = true;
    }
    /// Highlights the hovered slot (or none) and the bar fill while the pointer is inside.
    fn hover_dock(&mut self, slot: Option<usize>, inside: bool) {
        let Role::Dock { hover, slots, .. } = &mut self.role else {
            return;
        };
        let ease = |tween: &mut Tween, on: bool, curve| {
            let (target, duration) = if on {
                (
                    1.0,
                    if curve == motion::POP {
                        motion::POP_TIME
                    } else {
                        motion::HOVER_IN_TIME
                    },
                )
            } else {
                (0.0, motion::HOVER_OUT_TIME)
            };
            tween.retarget(target, duration, curve);
        };
        ease(&mut self.motion, inside, motion::SETTLE);
        for (index, tween) in slots.iter_mut().enumerate() {
            ease(tween, slot == Some(index), motion::POP);
        }
        *hover = slot;
        self.hover_unpainted = true;
        self.live = true;
    }
    fn paint_dock(
        &mut self,
        items: &[DockItem],
        icons: &[Vec<Option<Arc<IconPixels>>>],
        drag: Option<&Drag>,
        widget_readings: &crate::graphics::Readings,
    ) -> Result<()> {
        let now = Instant::now();
        let drag = drag.filter(|drag| drag.active);
        let (calm, rtl) = (motion::calm(), self.dock_buttons.rtl);
        let step = dock::px(dock::SLOT_DIPS, self.dpi) as f32 * if rtl { -1.0 } else { 1.0 };
        // A hover began at a mouse message; its first frame comes later. Start the clocks here so that frame
        // shows the rest value and the motion keeps its whole duration.
        if self.hover_unpainted {
            self.hover_unpainted = false;
            self.motion.restart(now);
            if let Role::Dock { slots, .. } = &mut self.role {
                slots.iter_mut().for_each(|tween| tween.restart(now));
            }
        }
        let Role::Dock { slots: tweens, .. } = &self.role else {
            return Ok(());
        };
        let count = items.len();
        let hover = |hit| {
            dock::cell_index(self.dock_buttons, count, hit)
                .and_then(|cell| tweens.get(cell))
                .map_or(0.0, |t| t.value(now))
        };
        let slots: Vec<DockSlot> = items
            .iter()
            .zip(icons)
            .enumerate()
            .map(|(index, (item, icon))| {
                // The held icon follows the pointer, lifted; the others slide to open the gap.
                let held = drag.is_some_and(|drag| drag.from == index);
                let dx = match drag {
                    Some(drag) if held => {
                        let (left, cell) = dock::span(
                            self.dock_buttons,
                            self.bounds.width,
                            self.dpi,
                            Hit::Slot(index),
                        );
                        (drag.at - left - cell / 2) as f32
                    }
                    Some(drag) if !calm => {
                        dock::drag_shift(drag.from, drag.gap.value(now), index) * step
                    }
                    _ => 0.0,
                };
                DockSlot {
                    icon: icon.first().and_then(Option::as_deref),
                    mini: if item.members.is_empty() {
                        Vec::new()
                    } else {
                        icon.iter().map(Option::as_deref).collect()
                    },
                    running: !item.windows.is_empty(),
                    windows: item.windows.len(),
                    active: item.active,
                    badge: item.badge,
                    faded: item.recent,
                    hover: if held { 1.0 } else { hover(Hit::Slot(index)) },
                    dx,
                    held,
                }
            })
            .collect();
        // Reduced motion: no sliding, a Lamp capsule on the edge where the icon would land.
        let marker = drag
            .filter(|drag| calm && drag.group.is_none() && drag.gap.target() != drag.from as f32)
            .map(|drag| {
                let slot = drag.gap.target() as usize;
                let (left, cell) = dock::span(
                    self.dock_buttons,
                    self.bounds.width,
                    self.dpi,
                    Hit::Slot(slot),
                );
                (if (slot > drag.from) != rtl {
                    left + cell
                } else {
                    left
                }) as f32
            });
        // Only the window rows from the content's top down are visible and take input: a region clips
        // the rest.
        let slide = dock::slide_offset(self.slide.value(now), self.bounds.height);
        let top = slide.min(self.bounds.height - 1);
        self.window.set_notch_region(&[Bounds {
            x: 0,
            y: top,
            width: self.bounds.width,
            height: self.bounds.height - top,
        }])?;
        self.canvas.paint_dock(&DockScene {
            marker,
            slots,
            layout: self.dock_buttons,
            widget_readings,
            buttons: Button::ALL
                .into_iter()
                .chain(Button::EXTRA)
                .chain(Button::WIDGETS)
                .filter(|button| self.dock_buttons.on(*button))
                .map(|button| (button, hover(Hit::Button(button))))
                .collect(),
            inside: self.motion.value(now),
            dpi: self.dpi,
            attached: self.dock_style == DockStyle::Attached,
            slide,
        })
    }
    /// The slot or button under the pointer.
    fn dock_hit(&self, lparam: LPARAM, slots: usize) -> Option<Hit> {
        let x = i32::from((lparam.0 & 0xffff) as u16 as i16);
        dock::hit_at(self.dock_buttons, x, self.bounds.width, self.dpi, slots)
    }
    /// The item a drag at window x lands on, and whether x is on that slot's middle (a folder drop).
    fn drop_at(&self, x: i32, items: usize, slots: usize) -> Option<(usize, bool)> {
        let (buttons, width, dpi) = (self.dock_buttons, self.bounds.width, self.dpi);
        let slot = dock::drop_index(buttons, x, width, dpi, slots, items)?;
        let (left, cell) = dock::span(buttons, width, dpi, Hit::Slot(slot));
        Some((slot, dock::folder_drop(x - left, cell, dpi)))
    }
    /// The pointer's cell index (the hover tween and UIA child), if over a slot or button.
    fn dock_cell(&self, lparam: LPARAM, slots: usize) -> Option<usize> {
        self.dock_hit(lparam, slots)
            .and_then(|hit| dock::cell_index(self.dock_buttons, slots, hit))
    }
}

/// Hover-intent swell: the pill grows ~11 x 3 DIP (the media pill ~7 x 6) to acknowledge the
/// pointer before opening.
const NOTCH_SWELL: f32 = 0.07;
/// In click mode the pointer entering swells the pill only when it is closed (its target is below the swell).
fn swells(target: f32) -> bool {
    target < NOTCH_SWELL
}
/// Opens the notch fully, ending any pending hover intent.
fn open_notch(intent: &mut Option<Instant>, tween: &mut Tween) {
    *intent = None;
    tween.retarget(1.0, motion::OPEN_TIME, motion::OPEN);
}
/// Client coordinates of a mouse message.
fn point(lparam: LPARAM) -> (i32, i32) {
    let x = i32::from((lparam.0 & 0xffff) as u16 as i16);
    let y = i32::from((lparam.0 >> 16) as u16 as i16);
    (x, y)
}
/// Brings one of our own popups (a menu) to the foreground so it can end when it loses it; Windows may
/// refuse, as for any app not in front.
fn take_foreground(notch: HWND) -> bool {
    // SAFETY: scalar foreground request for our own live notch HWND; never forced.
    unsafe { SetForegroundWindow(notch).as_bool() || GetForegroundWindow() == notch }
}
/// The notch volume bar eases to a new level over this long.
const VOLUME_EASE: Duration = Duration::from_millis(120);
/// After a swipe skips a track, further sideways wheel movement is ignored this long.
const SWIPE_PAUSE: Duration = Duration::from_millis(600);
/// Transient topology reads are retried this often, at most `TOPOLOGY_RETRIES` times per change.
const TOPOLOGY_RETRY: Duration = Duration::from_millis(250);
const TOPOLOGY_RETRIES: u32 = 8;
/// Windows sends no notification when a full-screen app starts or stops. The foreground geometry is
/// re-read on every catalog generation (foreground, show/hide, title, the foreground window's own resize,
/// and the 30 s repair scan) and topology change. The shell learns of a new full-screen window a moment
/// after it covers the monitor, so one more look is taken this long after a covering window was not
/// admitted; there is no timer otherwise.
const FULLSCREEN_POLL: Duration = Duration::from_secs(2);
/// A lost graphics device is rebuilt after a doubling delay (250 ms up to 8 s), since a driver reset
/// or update takes a moment. Any rebuild failure retries, as one during a reset need not read as a
/// loss; a failure ends the host once `GRAPHICS_ATTEMPTS` rebuilds have run with no settled minute
/// between losses. Repeated successful rebuilds (a flapping device) stay one per 8 s at most.
const GRAPHICS_RETRY: Duration = Duration::from_millis(250);
const GRAPHICS_ATTEMPTS: u32 = 8;
/// A loss this long after the last rebuild starts a fresh budget; sooner ones keep backing off.
const GRAPHICS_SETTLED: Duration = Duration::from_secs(60);
/// Visualizer samples are at least this far apart: at most 15 a second, even after a late wake.
const METER_PERIOD: Duration = Duration::from_millis(67);

/// When the visualizer samples next: never unless a visible notch shows bars (`open`: the Media panel or the pill), the
/// track plays and the endpoint has a meter; otherwise `METER_PERIOD` after the last sample, or at
/// once. The last sample outlives a stop, so stopping and restarting never samples faster.
fn meter_due(
    open: bool,
    playing: bool,
    metered: bool,
    sampled: Option<Instant>,
    now: Instant,
) -> Option<Instant> {
    (open && playing && metered).then(|| sampled.map_or(now, |at| at + METER_PERIOD))
}

fn rebuild_delay(attempts: u32) -> Duration {
    GRAPHICS_RETRY * (1 << attempts.min(5))
}
/// When the next device rebuild is due, and how many have run since the budget was refreshed.
struct Rebuild {
    due: Option<Instant>,
    attempts: u32,
    started: Instant,
}
impl Rebuild {
    fn new(now: Instant) -> Self {
        Self {
            due: None,
            attempts: 0,
            started: now,
        }
    }
    /// Schedules a rebuild for a detected loss; detections while one is pending keep it.
    fn lose(&mut self, now: Instant) {
        if self.due.is_some() {
            return;
        }
        if now.duration_since(self.started) >= GRAPHICS_SETTLED {
            self.attempts = 0;
        }
        self.due = Some(now + rebuild_delay(self.attempts));
    }
    /// Whether a rebuild is due now; if so it counts against the budget.
    fn start(&mut self, now: Instant) -> bool {
        if self.due.is_none_or(|due| due > now) {
            return false;
        }
        self.attempts += 1;
        self.started = now;
        true
    }
    /// Records a rebuild's outcome; false once a failure has spent the budget.
    fn finish(&mut self, rebuilt: bool, now: Instant) -> bool {
        if rebuilt {
            self.due = None;
        } else if self.attempts >= GRAPHICS_ATTEMPTS {
            return false;
        } else {
            self.due = Some(now + rebuild_delay(self.attempts));
        }
        true
    }
    fn wait(&self, now: Instant) -> u32 {
        self.due
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
}

/// `WM_MOUSEMOVE` wParam bit: the left button is still down.
const MK_LBUTTON: usize = 0x0001;

/// A press on a pinned dock slot: which pin, where the pointer went down, and whether it has moved far
/// enough to be a reorder (`dock::drag_started`) rather than a click.
struct Drag {
    from: usize,
    x: i32,
    active: bool,
    /// Pointer x now, the slot the gap eases to (held on a slot's middle, a folder drop), and that middle slot.
    at: i32,
    gap: Tween,
    group: Option<usize>,
}

/// Ends a dock drag, releasing the pointer capture its press took; no drag leaves any capture alone.
fn take_drag(drag: &mut Option<Drag>) -> Option<Drag> {
    let drag = drag.take()?;
    // SAFETY: releases this thread's own pointer capture; harmless if it is already gone.
    let _ = unsafe { ReleaseCapture() };
    Some(drag)
}

/// Applies one pin edit: only a real change (`edit` returns true) rewrites the store's pin section and marks
/// it dirty, once. Returns whether the dock must be rebuilt.
fn edit_pins(
    pins: &mut Pins,
    store: &mut Store,
    pending: &mut Pending,
    now: Instant,
    edit: impl FnOnce(&mut Pins) -> bool,
) -> bool {
    let changed = edit(pins);
    if changed {
        pins.write_to(store);
        pending.mark(now);
    }
    changed
}

/// Shelf page state, including the last removal available to undo.
struct ShelfView {
    removed: Option<(usize, shelf::Entry)>,
    shelf: Shelf,
    /// The Copied list and whether the Shelf page shows it instead of the files (not persisted).
    clips: Clips,
    copied: bool,
    /// The screenshot pill's stack (memory only).
    shots: crate::shots::Shots,
}

/// As `edit_pins`, only a real persisted change marks the store dirty.
fn edit_shelf(
    view: &mut ShelfView,
    store: &mut Store,
    pending: &mut Pending,
    now: Instant,
    edit: impl FnOnce(&mut Shelf) -> bool,
) -> bool {
    let changed = edit(&mut view.shelf);
    if changed && view.shelf.write_to(store) {
        pending.mark(now);
    }
    changed
}

/// The DPI icons are extracted for: the densest dock's, else the densest notch's (a shelf needs icons with
/// the dock off), else none.
fn icon_dpi(dock: Option<u32>, notch: Option<u32>) -> Option<u32> {
    dock.or(notch)
}

struct Desktop {
    surfaces: Vec<Surface>,
    /// Last settled topology; each monitor in it carries every shown kind.
    monitors: Vec<Monitor>,
    /// Kinds shown on every monitor; Show/Hide commands and closing a surface apply to all monitors.
    shown: Vec<SurfaceKind>,
    /// When the next topology read is due, and how many transient reads this change has retried.
    topology_due: Option<Instant>,
    topology_retries: u32,
    /// At most one validated full-screen window per settled monitor.
    fullscreen: Vec<(WindowKey, String)>,
    /// Catalog generation seen by the last full-screen check, and the next shell poll, armed only
    /// while a current or retained window covers its monitor.
    fullscreen_generation: Option<u64>,
    fullscreen_due: Option<Instant>,
    /// Dock auto-hide (`dock_tuck`), and the catalog generation it last read.
    tuck: dock_tuck::Tuck,
    tuck_notch: dock_tuck::Tuck,
    tuck_generation: Option<u64>,
    /// Keep awake from the quick settings is on (not stored: Windows drops it when ROQA quits).
    keep_awake: bool,
    /// A new song opened the notch on its own: when it closes again, and the song it showed.
    peek: Option<Instant>,
    peek_title: Option<String>,
    /// The Places popup's rows while it is open.
    places: Vec<crate::places::Place>,
    /// The identity of the drive whose arrival alert offers Eject (`drive_identified`).
    arrival: Option<crate::system_os::Volume>,
    /// The files a folder stack's popup lists while it is open.
    stack: Vec<std::path::PathBuf>,
    graphics: Graphics,
    rebuild: Rebuild,
    /// The reminder line box, open only while the user types into it.
    remind_box: Option<RemindBox>,
    /// The label of the reminder shown last (what Snooze sets again); the waiting ones are `pages.reminders`.
    last_reminder: String,
    /// The keyboard language and Caps Lock pills (`keys.rs`).
    keys: keys::Keys,
    /// The Windows notifications peek (`toast.rs`); none while its switch is off.
    toasts: Option<toast::Toasts>,
    /// The Windows taskbar hidden while its switch is on (`taskbar.rs`).
    taskbar: Option<crate::taskbar::Taskbar>,
    /// Open only while the user has it up; created on demand, destroyed on close, so it costs nothing closed.
    palette: Palette,
    palette_dirty: bool,
    settings_window: Option<SettingsWindow>,
    file_picker: shelf_os::FilePicker,
    /// What the user chose: loaded once, persisted through `store` on change, applied live by `apply_settings`.
    settings: Settings,
    dock_items: Vec<DockItem>,
    /// The display language reads right to left: the dock is mirrored (`dock::Buttons::rtl`).
    rtl: bool,
    dock_generation: Option<u64>,
    /// Windows that asked for attention (a taskbar flash) and have not been activated since; they wear the dot badge.
    flashing: Vec<usize>,
    /// The registered `SHELLHOOK` message, 0 when the shell would not take the hook window.
    shell_hook: u32,
    /// The notification-area icon on the wake window; declared first so it is removed before that window goes.
    tray: Option<crate::tray::Tray>,
    /// The tray menu put the dock and notch away until it shows them (or a show command) again.
    hidden: bool,
    /// The hidden window the workers' wakes and the Caps Lock hotkey go to, so a message box or menu loop dispatches them
    /// instead of dropping them (`native_window::post_wake`); it also takes the shell hook.
    wake: NativeWindow,
    /// Pinned apps, their persisted form, and the unwritten-change times. Read once at startup; written on
    /// a host-loop deadline (`store_wait`) after a change and once at quit, never by a timer or thread.
    pins: Pins,
    /// File references, drawn in the expanded notch; drops arrive through `drops` and are applied by
    /// `refresh_shelf`, so the drop target touches no host state.
    shelf: ShelfView,

    drops: Drops,
    /// A press on the screenshot chip or a kept shot, until it moves into a drag.
    shot_press: Option<shots::ShotPress>,
    /// An icon finished this pass (`refresh_dock` collected it).
    icons_arrived: bool,
    /// The dock icon table kept between the frames of one dock animation; dropped when the dock changes.
    animation_icons: Option<Vec<Vec<Option<Arc<IconPixels>>>>>,
    store: Store,
    store_path: Option<std::path::PathBuf>,
    pending: Pending,
    drag: Option<Drag>,
    /// Media session state, drawn by the notches. Declared before `text` so its WinRT objects and
    /// event tokens are released before that field's `RoUninitialize`, and before `icons`, whose
    /// worker stops only once media's handle to its queue is gone.
    media: Media,
    icons: IconService,
    /// The default render endpoint's volume, bound once a media notch is first shown.
    audio: Audio,
    /// The default recording device's mute (`mic.rs`), bound at start for the muted dot.
    mic: crate::mic::Mic,
    /// Windows text size (Accessibility > Text size) scales the whole notch, whose only content is text.
    text: TextSettings,
    text_factor: f64,
    /// Last drawn time text; the notches repaint only when it changes.
    clock: String,
    date: String,
    /// Just after the next minute boundary; until then a wake reuses the drawn text unformatted.
    clock_due: Instant,
    /// The next progress tick, and the scheduled time of the last one that came due: the 1 s
    /// floor counts from that, not from its late wake, so wake latency never accumulates.
    progress_due: Option<Instant>,
    progress_tick: Instant,
    /// When the visualizer last read the meter.
    sampled: Option<Instant>,
    /// Countdown timers: restored once from the store and driven only by `refresh_timers`' deadline.
    timers: Timers,
    /// The soonest timer's last requested panel text, so an expanded notch repaints only when it changes.
    timer_shown: String,
    /// The power pill: set by `message` on a status change, ended by `refresh_power`'s deadline.
    power: Power,
    /// Dock hover thumbnails: the pointer's rest on a slot, and the popup while one shows.
    preview: preview::Intent,
    /// A file drag resting on a dock slot: its window comes forward once (`dock::RAISE_DELAY`).
    raise: preview::Intent,
    popup: Option<preview::Popup>,
    /// The dock's right-click menu while open; nothing exists, and nothing is polled, otherwise.
    menu: Option<dock_menu::Menu>,
    hover_popup: Option<dock_menu::Menu>,
    hover_spot: Option<(usize, Hit, Instant)>,
    native_popup: crate::anchored::Native,
    pending_popup: Option<(crate::anchored::Owner, Instant)>,
    /// The dock button whose press just closed its own popup: its release ends that click (`swallow_release`).
    swallow_up: Option<(HWND, Button)>,
    /// Wheel movement left over after the last whole notch, one per control (`edge::Wheel`).
    wheel: [i32; 3],
    /// Sideways wheel movement kept toward a track skip, and when the next skip is allowed (one skip per swipe).
    swipe: i32,
    swipe_until: Instant,
    /// Recently closed apps the dock keeps faded at its end, newest first; in memory only (`dock::with_recent`).
    recent: Vec<DockItem>,
    /// The open volume or brightness popup's state, and the brightness worker it shares (`host/levels.rs`).
    levels: Option<levels::Levels>,
    center: command_center::Center,
    brightness: Option<crate::brightness::Brightness>,
    /// The tabs and the mascot (features tabs-eyes).
    pages: Pages,
    /// The weather place, report and back-off, the worker channel and the locale's region (features W1).
    weather: Feed,
    weather_io: weather_os::Io,
    region: String,
    /// The plan limits, their worker channel and whether the page was open at the last pass (features U1).
    limits: Limits,
    limits_io: limits_os::Io,
    limits_open: bool,
    /// The prayer reminders' schedule and sound (features P1).
    prayer: PrayerRun,
    /// What coding agents reported (`roqa.exe agent`): the pill text and the eyes' mood.
    agents: agent::Agents,
    /// The registered global shortcuts and the dock's hidden-by-shortcut state (this run only).
    hotkeys: hotkeys::Hotkeys,
    /// The hour whose coming rain the notch already announced (`weather::rain_soon`), so it says it once.
    rain_told: Option<u8>,
    dock_away: bool,
    /// When the next water reminder is due; `None` until `refresh_water` computes it.
    water_due: Option<Instant>,
}
/// The reminder schedule for the day, the last pass it was checked at, and the MCI sound while one plays.
#[derive(Default)]
struct PrayerRun {
    /// The facts `events` were computed for: date, UTC offset, place, method and heads-up minutes.
    key: Option<(
        prayer_tab::Local,
        i32,
        i32,
        crate::prayer::Method,
        u32,
        [i8; 6],
    )>,
    events: Vec<prayer_tab::Event>,
    last: Option<((i32, u32, u32), i32)>,
    player: prayer_os::Player,
    sound_end: Option<Instant>,
    /// The reminder's notice ends here: the mascot's adhan mood lasts as long.
    adhan_until: Option<Instant>,
}
/// How long a prayer reminder holds the notch open.
const PRAYER_NOTICE: Duration = Duration::from_secs(8);
/// A show command brings an auto-hidden dock or notch back for this long (the pointer on it keeps it).
const SHOW_COMMAND: Duration = Duration::from_secs(6);
/// How long a keyboard pill holds the notch open.
const KEY_NOTICE: Duration = Duration::from_secs(2);
/// How long a device alert holds the notch open.
const DEVICE_NOTICE: Duration = Duration::from_secs(4);
/// How long a drive alert holds the notch open: long enough to press Eject.
const DRIVE_NOTICE: Duration = Duration::from_secs(8);
/// How long a progress report holds the notch open (the next report renews it), and a finished one.
const PROGRESS_HOLD: Duration = Duration::from_secs(10);
const PROGRESS_DONE: Duration = Duration::from_secs(2);
/// How long a volume change from outside ROQA holds the notch open.
const VOLUME_NOTICE: Duration = Duration::from_millis(1500);
/// The system sound for a finished timer or a due reminder (callers check the sound settings).
fn chime() {
    // SAFETY: plays the user's own default sound asynchronously; no pointers.
    unsafe {
        let _ = MessageBeep(MB_ICONASTERISK);
    }
}
impl Desktop {
    fn create(_probe: &Probe) -> Result<Self> {
        let monitors = topology::settle(native_window::monitors()?)
            .ok_or_else(|| Error::new(E_FAIL, "No usable display monitor."))?;
        let text = TextSettings::create()?;
        let text_factor = text.factor()?;
        let (mut store, store_path) = store::open(store::default_path());
        let now = Instant::now();
        let devices: Vec<String> = monitors.iter().map(|m| m.device.clone()).collect();
        let settings = store.settings.clone().keep_one_monitor(&devices);
        settings.apply_globals();
        let mut shelf = Shelf::from_store(&store);
        if shelf.set_cap(settings.shelf_cap) {
            shelf.write_to(&mut store); // the cut list leaves the file with its next write
        }
        let clips = Clips::default();
        let mut pages = Pages::new(&settings, store.last_tab);
        let weather = Feed::new(store.place.clone(), store.report.clone(), settings.detect);
        let timers = Timers::restore(&store.timers, timer::wall_now(), now);
        // One that came due while ROQA was not running shows at the first pass.
        pages.reminders = remind::Reminders::from_store(&store);
        // The section now mirrors what is running; stale entries leave the file with its next write.
        timers.write_to(&mut store, &mut Pending::default(), now);
        let effective_accent = if settings.match_accent {
            crate::theme::windows_accent_color().or_else(|| Some(text.accent()))
        } else {
            None
        };
        let wake = NativeWindow::create_hidden(
            layout::Bounds {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            w!("ROQA Wake"),
        )?;
        native_window::set_wake_window(wake.0);
        let tray = crate::tray::Tray::add(wake.0);
        let mut desktop = Self {
            surfaces: Vec::new(),
            monitors,
            shown: Vec::new(),
            topology_due: None,
            topology_retries: 0,
            fullscreen: native_window::fullscreen_candidate()
                .filter(|(key, _)| {
                    settings.hide_fullscreen && native_window::shell_fullscreen(*key)
                })
                .into_iter()
                .collect(),
            fullscreen_generation: None,
            fullscreen_due: None,
            tuck: dock_tuck::Tuck::default(),
            tuck_notch: dock_tuck::Tuck::top(),
            tuck_generation: None,
            keep_awake: false,
            peek: None,
            peek_title: None,
            places: Vec::new(),
            arrival: None,
            stack: Vec::new(),
            graphics: Graphics::create()?,
            rebuild: Rebuild::new(Instant::now()),
            remind_box: None,
            last_reminder: String::new(),
            keys: keys::Keys::new(),
            toasts: settings.notifications.then(toast::Toasts::new),
            taskbar: settings.hide_taskbar.then(crate::taskbar::Taskbar::hide),
            palette: crate::theme::system_palette(settings.theme, effective_accent),
            palette_dirty: false,
            settings_window: None,
            file_picker: Default::default(),
            settings,
            icons: IconService::create()?,
            dock_items: Vec::new(),
            rtl: dock::system_rtl(),
            dock_generation: None,
            flashing: Vec::new(),
            shell_hook: 0,
            tray,
            hidden: false,
            wake,
            pins: Pins::from_store(&store),
            shelf: ShelfView {
                removed: None,
                shelf,
                clips,
                copied: false,
                shots: crate::shots::Shots::default(),
            },
            drops: Drops::default(),
            shot_press: None,
            icons_arrived: false,
            animation_icons: None,
            store,
            store_path,
            pending: Pending::default(),
            drag: None,
            media: Media::create(),
            audio: Audio::create(),
            mic: crate::mic::Mic::start(),
            text,
            text_factor,
            clock: String::new(),
            date: String::new(),
            clock_due: Instant::now(),
            progress_due: None,
            progress_tick: Instant::now(),
            sampled: None,
            timers,
            timer_shown: String::new(),
            power: Power::new(power::Raw::read()),
            preview: preview::Intent::default(),
            raise: preview::Intent::default(),
            popup: None,
            menu: None,
            hover_popup: None,
            hover_spot: None,
            native_popup: Default::default(),
            pending_popup: None,
            swallow_up: None,
            wheel: [0; 3],
            swipe: 0,
            swipe_until: Instant::now(),
            levels: None,
            center: Default::default(),
            brightness: None,
            recent: Vec::new(),
            pages,
            weather,
            weather_io: weather_os::Io::new(),
            region: weather_os::region(),
            limits: Limits::default(),
            limits_io: limits_os::Io::new(),
            limits_open: false,
            prayer: PrayerRun::default(),
            agents: agent::Agents::default(),
            hotkeys: hotkeys::Hotkeys::default(),
            rain_told: None,
            dock_away: false,
            water_due: None,
        };
        desktop.hotkeys.sync(desktop.settings.hotkeys);
        if desktop.settings.notch {
            desktop.show(SurfaceKind::Notch)?;
        }
        #[cfg(debug_assertions)]
        if *_probe == Probe::Initialization {
            return Err(Error::new(
                E_FAIL,
                "Injected failure after first composed window.",
            ));
        }
        if desktop.settings.dock {
            desktop.show(SurfaceKind::Dock)?;
        }
        // After the first paints, so loading the media component never delays them. WinRT is
        // already initialized on this thread by `TextSettings`.
        desktop.media.start(desktop.icons.jobs());
        Ok(desktop)
    }
    fn dock_slots(&self) -> usize {
        self.dock_items.len()
    }
    /// The chosen buttons with the system reading direction applied.
    fn dock_buttons(&self) -> Buttons {
        Buttons {
            rtl: self.rtl,
            battery: true,
            limits: self.settings.tabs[Tab::Limits.index()],
            moon: self.settings.dock_moon.then(|| self.pages.moon.icon()),
            volume: self.settings.edge_volume,
            brightness: self.settings.edge_brightness,
            widgets: self.visible_dock_widgets(),
            centre: if self.settings.dock_left {
                0
            } else {
                self.dock_slots()
            },
            ..self.settings.dock_buttons
        }
    }
    fn show(&mut self, kind: SurfaceKind) -> Result<()> {
        if !self.shown.contains(&kind) {
            self.shown.push(kind);
        }
        for monitor in self.monitors.clone() {
            self.place(&monitor, kind)?;
        }
        Ok(())
    }
    /// Creates and paints `kind` on `monitor` unless it already has one.
    fn place(&mut self, monitor: &Monitor, kind: SurfaceKind) -> Result<()> {
        if self.settings.off_monitors.contains(&monitor.device) {
            return Ok(()); // the user switched ROQA off on this monitor
        }
        if self
            .surfaces
            .iter()
            .any(|surface| surface.kind() == kind && surface.monitor == monitor.device)
        {
            return Ok(());
        }
        let dpi = match kind {
            // The notch's effective DPI: monitor DPI times the Windows text-size factor.
            SurfaceKind::Notch => layout::notch_dpi(monitor.work, monitor.dpi, self.text_factor),
            SurfaceKind::Dock => monitor.dpi,
        };
        let mut surface = Surface::create(
            &self.graphics,
            kind,
            monitor,
            dpi,
            self.dock_slots(),
            &self.drops,
            self.palette,
            self.settings.dock_style,
            self.dock_buttons()
                .fit_widgets(monitor.dock_room(self.settings.dock_style).width, dpi),
        )?;
        if kind == SurfaceKind::Notch {
            // Advances every existing notch first, so all monitors show the same minute.
            self.refresh_clock()?;
            surface.paint_notch(
                (&self.clock, &self.date),
                &self.media,
                &self.audio,
                &self.timers,
                &self.power,
                &self.shelf,
                &self.pages,
            )?;
            // Always on: a screenshot's image starts the pill; text joins Copied only when that is on.
            clipboard_os::watch(surface.window.0, true);
            self.surfaces.push(surface);
        } else {
            self.surfaces.push(surface);
            // Sized after the push: icons are extracted once, for the densest dock.
            let icons = self.dock_icons();
            let last = self.surfaces.len() - 1;
            self.surfaces[last].paint_dock(
                &self.dock_items,
                &icons,
                self.drag.as_ref(),
                &self.pages.widget_readings,
            )?;
        }
        // Shown after its first paint, and never on the monitor a full-screen app covers.
        let visible = !self.hidden
            && !self
                .fullscreen
                .iter()
                .any(|(_, device)| device == &monitor.device)
            && match kind {
                SurfaceKind::Dock => self.tuck.shown(&monitor.device),
                SurfaceKind::Notch => self.tuck_notch.shown(&monitor.device),
            };
        let last = self.surfaces.len() - 1;
        self.surfaces[last].set_visible(visible);
        Ok(())
    }
    /// Formats the time only once the minute may have changed, and repaints the notches only when the
    /// text changed; returns how long until the next minute.
    // Windows `Instant` uses QPC, including suspend; an elapsed deadline is detected on the next loop pass.
    fn refresh_clock(&mut self) -> Result<u32> {
        let now = Instant::now();
        if now >= self.clock_due {
            let clock = clock::now(self.pages.tabs.on(Tab::Prayer));
            self.clock_due = now + Duration::from_millis(clock.next_minute_ms.into());
            self.pages.hour = clock.hour;
            self.pages.hijri = clock.hijri;
            self.rain_soon(clock.hour, clock.minute);
            self.refresh_moon()?;
            if clock.time != self.clock || clock.date != self.date {
                (self.clock, self.date) = (clock.time, clock.date);
                self.request_notches(Repaint::Full, |_| true);
            }
        }
        Ok(milliseconds_until(self.clock_due, now))
    }
    /// Asks for `repaint` on every visible notch `wanted` accepts. Nothing is drawn until the pass
    /// ends (`paint_requested`); hidden notches are painted before they are restored.
    fn request_notches(&mut self, repaint: Repaint, wanted: impl Fn(&Surface) -> bool) {
        for notch in &mut self.surfaces {
            if notch.kind() == SurfaceKind::Notch && notch.visible && wanted(notch) {
                notch.request(repaint);
            }
        }
    }
    /// Paints each visible notch once for whatever this pass asked of it.
    fn paint_requested(&mut self) -> Result<()> {
        let peaks = self.audio.peaks();
        for notch in &mut self.surfaces {
            match std::mem::take(&mut notch.repaint) {
                _ if !notch.visible => {}
                Repaint::Nothing => {}
                Repaint::Meter => notch.paint_meter(peaks)?,
                Repaint::Full => notch.paint_notch(
                    (&self.clock, &self.date),
                    &self.media,
                    &self.audio,
                    &self.timers,
                    &self.power,
                    &self.shelf,
                    &self.pages,
                )?,
            }
        }
        Ok(())
    }
    /// Repaints each open notch whose progress row changed, such as after a seek, and returns how
    /// long until a playing track's row should tick next (`Track::next_tick`, with its 1 s floor).
    /// Collapsed and hidden notches cost nothing here.
    // Out of line: inlined into `update`, it cost about 12 KB more release binary (measured).
    #[inline(never)]
    fn refresh_progress(&mut self) -> Result<u32> {
        let mut next_due = None::<Instant>;
        let media_page = self.page() == Tab::Media;
        // A visible collapsed notch ticks too: its pill shows the elapsed time while a track plays.
        for notch in self
            .surfaces
            .iter_mut()
            .filter(|s| s.visible && s.kind() == SurfaceKind::Notch)
        {
            let open = notch.expanded();
            let Some(track) = self
                .pages
                .track(&self.media)
                .filter(|track| (media_page || !open) && (open || track.playing))
            else {
                continue;
            };
            let (now, ticks) = (Instant::now(), media::now_ticks());
            if let Some(due) = self.progress_due.filter(|&due| due <= now) {
                self.progress_tick = due;
            }
            let width = if open {
                layout::media_progress(notch.dpi).width
            } else {
                1
            };
            // A running animation paints every frame already.
            if !notch.live && notch.played != track.progress(ticks, width) {
                notch.request(Repaint::Full);
            }
            let floor =
                (self.progress_tick + Duration::from_secs(1)).saturating_duration_since(now);
            if let Some(wait) = track.next_tick(ticks, width, floor) {
                let due = now + wait;
                next_due = Some(next_due.map_or(due, |next| next.min(due)));
            }
        }
        self.progress_due = next_due;
        Ok(next_due.map_or(INFINITE, |due| milliseconds_until(due, Instant::now())))
    }
    /// Reads the endpoint meter once per due sample (`meter_due`) and asks to repaint the meter slot
    /// of each settled open notch whose bar heights changed; returns how long until the next sample. Stopped, it makes
    /// no meter call, flattens the bars and has no deadline, and with no open notch paints nothing.
    #[inline(never)]
    fn refresh_meter(&mut self) -> Result<u32> {
        let now = Instant::now();
        let track = self.pages.track(&self.media);
        let (shown, playing) = (track.is_some(), track.is_some_and(|track| track.playing));
        // The open panel draws the visualizer on the Media page only; a visible collapsed pill shows
        // the playing track's level bars on any page.
        let media_page = self.page() == Tab::Media;
        let open = self
            .surfaces
            .iter()
            .any(|s| s.visible && s.kind() == SurfaceKind::Notch && (media_page || !s.expanded()));
        let due = meter_due(open, playing, self.audio.metered(), self.sampled, now);
        match due {
            None => self.audio.stop(),
            Some(due) if due > now => return Ok(milliseconds_until(due, now)),
            Some(_) => {
                self.audio.sample();
                self.sampled = Some(now);
            }
        }
        let peaks = self.audio.peaks();
        for notch in self
            .surfaces
            .iter_mut()
            .filter(|s| s.visible && s.kind() == SurfaceKind::Notch)
        {
            // A running animation paints every frame already. The pill has no slot to patch: it repaints whole.
            if shown
                && !notch.live
                && (media_page || !notch.expanded())
                && notch.bars != layout::meter_heights(peaks, notch.dpi)
            {
                notch.request(if notch.expanded() {
                    Repaint::Meter
                } else {
                    Repaint::Full
                });
            }
        }
        Ok(due.map_or(INFINITE, |_| milliseconds_until(now + METER_PERIOD, now)))
    }
    /// Completes due timers, ends a finished alert, and asks an expanded notch to repaint when the
    /// soonest timer's seconds changed; returns the wait until the timers' next deadline. With no timers
    /// and no alert it does nothing and returns `INFINITE`: no wake, and collapsed notches get no tick.
    #[inline(never)]
    fn refresh_timers(&mut self) -> u32 {
        if !self.pages.tabs.on(Tab::Timer) && !self.settings.dock_buttons.widgets[5] {
            return INFINITE; // switched off: no deadline, no alert, nothing to tick
        }
        let now = Instant::now();
        if self.timers.take_done(now) {
            self.timers
                .write_to(&mut self.store, &mut self.pending, now);
            self.complete_timer();
            if self.timers.take_pomo_done() {
                // The next phase follows by itself; Start or pause stops the round.
                let finished = self.pages.pomo.phase.label();
                self.pages.pomo.advance(&self.settings.pomo_cfg);
                let status = self.pomo_start(now);
                self.show_notice(format!("{finished} done. {status}"), now);
            }
            self.request_notches(Repaint::Full, |_| true);
        }
        if self.timers.alert_over(now) {
            self.surfaces.iter_mut().for_each(Surface::close_alert);
            self.request_notches(Repaint::Full, |_| true);
        }
        let expanded = self.page() == Tab::Timer && self.surfaces.iter().any(Surface::expanded);
        if expanded {
            let panel = self
                .pages
                .timer_view(&self.timers, now)
                .map(|view| view.panel);
            if panel.as_deref().unwrap_or("") != self.timer_shown {
                self.timer_shown = panel.unwrap_or_default();
                // A running animation paints every frame already.
                self.request_notches(Repaint::Full, |s| s.expanded() && !s.live);
            }
        }
        self.timers
            .next_wake(expanded || self.dock_timer_ticks(), now)
            .into_iter()
            .chain(self.pages.stopwatch.next_tick(now).filter(|_| expanded))
            .min()
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// The wheel over the open notch: `by` tabs along the bar (positive is later), stopping at the ends.
    fn flip_tab(&mut self, by: i32) {
        let visible = self.pages.tabs.visible();
        let Some(at) = visible.position(self.page()) else {
            return;
        };
        let to = (at as i32 + by).clamp(0, visible.as_slice().len() as i32 - 1);
        if let Some(&tab) = visible.as_slice().get(to as usize) {
            self.select_tab(tab);
        }
    }
    /// The page the open panel shows now.
    fn page(&self) -> Tab {
        self.pages.page(self.pages.track(&self.media).is_some())
    }
    /// A pick for the Media session's deadline only while the Media tab is on.
    fn media_wait(&self) -> u32 {
        if !self.pages.tabs.on(Tab::Media) {
            return INFINITE;
        }
        self.media
            .deadline()
            .map_or(INFINITE, |due| milliseconds_until(due, Instant::now()))
    }
    /// The user (or UI Automation, or a finished timer) picked `tab`: it shows now, becomes the last used
    /// tab (kept in the store) and retires older UI Automation elements.
    fn select_tab(&mut self, tab: Tab) {
        let live = self.pages.track(&self.media).is_some();
        if !self.pages.tabs.select(tab, live) {
            return;
        }
        if self.store.last_tab != tab {
            self.store.last_tab = tab;
            self.pending.mark(Instant::now());
        }
        self.request_notches(Repaint::Full, |_| true);
    }
    /// Weather (features W1): takes a finished worker's answer, starts a job when `Feed::wants` says so and
    /// keeps the page text current. It sets no deadline of its own: the minute clock's wake runs it, so a due
    /// refresh lands within a minute, and a switched-off tab costs nothing but this early return.
    #[inline(never)]
    fn refresh_weather(&mut self) {
        let now = Instant::now();
        while let Some((id, outcome)) = self.weather_io.poll() {
            if self.weather.finish(id, outcome, now) {
                self.store.place = self.weather.place.clone();
                self.store.report = self.weather.report.clone();
                self.pending.mark(now);
            }
        }
        self.weather.expire(now);
        let (on, prayer) = (
            self.pages.tabs.on(Tab::Weather) || self.visible_dock_widgets()[0],
            self.pages.tabs.on(Tab::Prayer) || self.visible_dock_widgets()[1],
        );
        if !weather::feed_wanted(on, prayer, self.weather.place.is_some()) {
            return; // no fetch, no worker, no page
        }
        let shown = self.page() == Tab::Weather || self.visible_dock_widgets()[0];
        let (detect, configured) = (
            self.settings.detect,
            weather::configured(self.settings.detect, &self.settings.city),
        );
        let ctx = weather::Ctx {
            enabled: true,
            configured,
            visible: self.surfaces.iter().any(|s| {
                s.visible && (s.kind() == SurfaceKind::Notch || self.visible_dock_widgets()[0])
            }),
            open: shown
                && (self.surfaces.iter().any(Surface::expanded) || self.visible_dock_widgets()[0]),
            wall: timer::wall_now(),
        };
        let idle = || weather_os::idle_secs() >= weather_os::IDLE_AFTER_SECS;
        if self.weather.wants(&ctx, now, idle) {
            let job = self.weather.start(now, detect, &self.settings.city);
            let id = job.id;
            if !self.weather_io.spawn(job) {
                self.weather.request_failed(id, now);
            }
        }
        // The text is rebuilt when the data or the minute (the age) changed, and only while it shows.
        let current = |key: &Option<(u32, String)>| {
            key.as_ref()
                .is_some_and(|(rev, clock)| *rev == self.weather.rev && *clock == self.clock)
        };
        let reads = shown || self.page() == Tab::Media; // and for the Media page glance (`tabs::glance`)
        if reads && self.pages.tabs.on(Tab::Weather) && !current(&self.pages.weather_key) {
            let fahrenheit = self.settings.units.fahrenheit(&self.region);
            self.pages.weather = weather::view(
                &self.weather,
                configured,
                fahrenheit,
                ctx.wall,
                &weather_os::hour_label,
            );
            self.pages.weather_key = Some((self.weather.rev, self.clock.clone()));
            self.request_notches(Repaint::Full, |s| motion::reveal(s.motion.value(now)) > 0.0);
        }
    }
    /// Limits (features U1): takes a finished worker's answer, starts one job when the dock's Limits popup newly
    /// opens (the worker reads only what changed or aged out), and keeps its rows current while it shows. No
    /// timer and no watcher: a closed popup costs nothing but this early return.
    #[inline(never)]
    fn refresh_limits(&mut self) {
        self.limits.configure(
            (self.settings.limits_codex, self.settings.limits_agy),
            self.settings.limits_group,
        );
        let now = limits_os::now();
        while let Some(outcome) = self.limits_io.poll() {
            self.limits.finish(outcome, now);
        }
        let open = self.limits_popup();
        if !open {
            self.limits_io.cancel();
        }
        if open && !self.limits_open {
            self.start_limits(now, false);
        }
        self.limits_open = open;
        if !open {
            self.pages.limits_key = None;
            return;
        }
        let key = (self.limits.rev, self.limits.busy(), self.clock.clone());
        if self.pages.limits_key.as_ref() != Some(&key) {
            self.pages.limits = limits::view(
                &self.limits.rows,
                now,
                &limits::clock(prayer_os::local_now().offset_min),
                key.1,
            );
            self.pages.limits_key = Some(key);
            let (items, titles) = self.limits_items();
            if let Some(popup) = &mut self.hover_popup
                && popup.relist(items, titles, Vec::new()).is_err()
            {
                self.hover_popup = None;
            }
        }
    }
    /// Whether the hover popup showing is the dock's Limits one.
    fn limits_popup(&self) -> bool {
        self.hover_popup.as_ref().is_some_and(|popup| {
            popup.hover_hit == Some(Hit::Button(Button::Limits)) && !popup.closing()
        })
    }
    /// The Limits popup's rows from the current view: a bar per window, text for the rest.
    fn limits_items(&self) -> (Vec<Item>, Vec<String>) {
        limits::popup_rows(&self.pages.limits)
            .into_iter()
            .map(|(text, bar)| (bar.map_or(Item::Label, Item::Limit), text))
            .unzip()
    }
    /// A job for the Limits worker (`Limits::start` says no inside the cooldown or while one runs).
    fn start_limits(&mut self, now: i64, manual: bool) {
        let allow = (self.settings.limits_codex, self.settings.limits_agy);
        if let Some(job) = self
            .limits
            .start(now, manual, allow, self.settings.limits_group)
            && !self.limits_io.spawn(job)
        {
            // No thread: release the claim so a later open can try again.
            self.limits.finish(
                limits::Outcome {
                    id: job.id,
                    ..Default::default()
                },
                now,
            );
        }
    }
    /// The mascot: picks the mood from the facts and steps the running show, or starts the idle one whose
    /// deadline passed. With the setting off it returns at once. Returns the wait until the next step or show.
    #[inline(never)]
    fn refresh_eyes(&mut self) -> u32 {
        let now = Instant::now();
        self.agents.prune(now);
        let facts = eyes::Facts {
            agent: self.agents.state(now),
            playing: self
                .pages
                .track(&self.media)
                .is_some_and(|track| track.playing),
            alert: self.power.text().is_some()
                || (self.pages.tabs.on(Tab::Timer) && self.timers.finished()),
            idle: now.saturating_duration_since(self.pages.last_input) >= eyes::IDLE_AFTER,
            adhan: self.prayer.adhan_until.is_some_and(|end| now < end),
            rain: self.pages.page(self.pages.track(&self.media).is_some()) == Tab::Weather
                && self.weather.report.as_ref().is_some_and(|report| {
                    matches!(
                        weather::Icon::from_wmo(report.code),
                        weather::Icon::Rain | weather::Icon::Storm
                    )
                }),
            hour: self.pages.hour,
        };
        let lean = if self.timers.running() {
            7.0
        } else if facts.playing {
            -8.0
        } else {
            0.0
        };
        let Some(eyes) = &mut self.pages.eyes else {
            return INFINITE;
        };
        eyes.set_lean(lean);
        let changed = eyes.set_mood(eyes::mood(facts), now) | eyes.tick(now);
        let due = self
            .agents
            .deadline()
            .map_or(eyes.due(), |end| end.min(eyes.due()));
        if changed {
            // A running animation paints every frame already; an open panel does not draw the eyes.
            self.request_notches(Repaint::Full, |s| !s.live && s.motion.value(now) < 0.1);
        }
        milliseconds_until(due, now)
    }
    /// The collapsed pill's timer ring breathes: while a countdown runs and a pill is on screen, closed and not
    /// already animating, it is repainted every `BREATH_STEP`. Hidden, open, reduced motion or no timer: no wait.
    #[inline(never)]
    fn refresh_ring(&mut self) -> u32 {
        if motion::calm()
            || !self.pages.tabs.on(Tab::Timer)
            || !(self.timers.running() || self.pages.stopwatch.running())
            || self.pages.track(&self.media).is_some()
        {
            return INFINITE;
        }
        let now = Instant::now();
        let mut shown = false;
        for notch in &mut self.surfaces {
            if notch.kind() == SurfaceKind::Notch && notch.visible && notch.collapsed(now) {
                shown = true;
                if !notch.live {
                    notch.request(Repaint::Full);
                }
            }
        }
        if shown {
            motion::BREATH_STEP.as_millis() as u32
        } else {
            INFINITE
        }
    }
    /// Steps the dots' fade; returns the wait until its next step (`INFINITE` at rest, the common case).
    #[inline(never)]
    fn refresh_privacy(&mut self) -> u32 {
        let now = Instant::now();
        if self.pages.dots.tick(now) {
            self.request_notches(Repaint::Full, |_| true);
        }
        self.pages
            .dots
            .due()
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// The consent store changed: re-arm first so no later change is lost, then read it once.
    fn privacy_signal(&mut self) {
        let Some(watch) = &self.pages.watch else {
            return;
        };
        watch.arm();
        if self.pages.dots.set(watch.read(), Instant::now()) {
            self.request_notches(Repaint::Full, |_| true);
        }
    }
    /// A new song while every notch is closed opens them for `PEEK_TIME`; the first song after start does not.
    fn peek_song(&mut self) {
        let Some(title) = self
            .pages
            .track(&self.media)
            .filter(|track| track.playing)
            .map(|track| track.title.clone())
        else {
            return;
        };
        if self.peek_title.as_ref() == Some(&title) {
            return;
        }
        let first = self.peek_title.replace(title).is_none();
        let now = Instant::now();
        if first
            || !self
                .surfaces
                .iter()
                .filter(|s| s.kind() == SurfaceKind::Notch)
                .all(|s| s.collapsed(now))
        {
            return;
        }
        self.open_notches();
        self.peek = Some(now + motion::PEEK_TIME);
    }
    /// Opens every visible notch; a notch hidden by a full-screen app stays hidden.
    fn open_notches(&mut self) {
        self.peek_tucked_notches();
        for surface in &mut self.surfaces {
            if surface.visible
                && let Role::Notch { intent, linger, .. } = &mut surface.role
            {
                linger.clear();
                open_notch(intent, &mut surface.motion);
                surface.live = true;
            }
        }
    }
    fn peek_tucked_notches(&mut self) {
        // An alert or a new song shows a tucked notch for a while, like Bloom's peek.
        let now = Instant::now();
        let hidden: Vec<_> = self
            .surfaces
            .iter()
            .filter(|s| s.kind() == SurfaceKind::Notch && !s.visible)
            .map(|s| s.monitor.clone())
            .collect();
        let peeked = hidden.iter().fold(false, |any, device| {
            self.tuck_notch
                .alert(device, now, motion::time(motion::PEEK_TIME))
                || any
        });
        if peeked {
            // A paint failure leaves the notch hidden; the next pass retries.
            let _ = self.apply_visibility();
        }
    }
    /// Shows a delayed DWM thumbnail, held through pointer transfer and popup interaction.
    #[inline(never)]
    fn refresh_preview(&mut self) -> u32 {
        let now = Instant::now();
        let slots = self.dock_slots();
        let hovered = self
            .surfaces
            .iter()
            .enumerate()
            .find_map(|(index, surface)| {
                let Role::Dock {
                    hover: Some(cell), ..
                } = surface.role
                else {
                    return None;
                };
                // Buttons have no windows; a pinned app that is not running has nothing to show.
                let slot = dock::cell(surface.dock_buttons, slots, cell)?
                    .slot()
                    .filter(|_| surface.visible)?;
                self.dock_items
                    .get(slot)
                    .filter(|item| !item.windows.is_empty() && item.members.is_empty())
                    .map(|item| (index, slot, item))
            });
        // A thumbnail would float over the open menu.
        let hovered = hovered.filter(|_| self.menu.is_none());
        let spot = hovered.map(|(index, slot, _)| (self.surfaces[index].window.0.0 as usize, slot));
        let over = self.popup.as_ref().is_some_and(|p| p.under_pointer());
        let ended = self.preview.point_preview(spot, over, now);
        let stale = self.popup.as_ref().is_some_and(|popup| {
            let owner = self.preview.spot().and_then(|(hwnd, slot)| {
                self.dock_at(HWND(hwnd as *mut _)).filter(|s| s.visible)?;
                self.dock_items.get(slot)
            });
            self.menu.is_some() || !owner.is_some_and(|item| item.windows.contains(&popup.target))
        });
        if ended || stale {
            self.popup = None;
            if !ended {
                self.preview.dismiss(); // the window went away under a resting pointer
            }
        }
        if self.preview.take_due(now).is_some() {
            let shown = hovered.and_then(|(index, slot, item)| {
                let dock = &self.surfaces[index];
                let work = self
                    .monitors
                    .iter()
                    .find(|m| m.device == dock.monitor)?
                    .work;
                let theme = self.palette;
                let slot_x = dock::slot_center_x(
                    dock.bounds.x,
                    dock.dock_buttons,
                    dock.bounds.width,
                    slot,
                    dock.dpi,
                );
                preview::Popup::show(&item.windows, theme, slot_x, dock.bounds, work, dock.dpi).ok()
            });
            match shown {
                Some(popup) => self.popup = Some(popup),
                None => {
                    self.preview.dismiss();
                }
            }
        }
        self.preview
            .deadline()
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// A click on the dock ends the preview; it stays away until the pointer moves to another slot.
    fn dismiss_preview(&mut self) {
        self.popup = None;
        self.preview.dismiss();
    }
    /// A timer finished: every visible notch opens (once, however many finished together) and the
    /// system sound plays if enabled. No toast: notification packaging is M6.
    fn complete_timer(&mut self) {
        if self.pages.tabs.on(Tab::Timer) {
            // The open it causes shows the finished timer, whatever page was last used.
            self.pages.tabs.choice = Some(Tab::Timer);
        }
        self.open_notches();
        if self.settings.timer_beeps() {
            chime();
        }
    }
    /// Paints one frame of each running animation; returns true while any still runs.
    /// The final frame always lands exactly on the target before a surface goes idle.
    fn animate(&mut self) -> Result<bool> {
        let now = Instant::now();
        let docks_live = self
            .surfaces
            .iter()
            .any(|s| s.live && s.kind() == SurfaceKind::Dock);
        let icons = if docks_live {
            match self.animation_icons.take() {
                Some(icons) => icons,
                None => self.dock_icons(),
            }
        } else {
            Vec::new()
        };
        let mut running = self
            .hover_popup
            .as_ref()
            .is_some_and(|popup| popup.running(now));
        let mut settled = false;
        for surface in &mut self.surfaces {
            if let Role::Notch { intent, .. } = &mut surface.role
                && intent.is_some_and(|at| now >= at)
            {
                open_notch(intent, &mut surface.motion);
            }
            if surface.live {
                match surface.kind() {
                    SurfaceKind::Notch => surface.paint_notch(
                        (&self.clock, &self.date),
                        &self.media,
                        &self.audio,
                        &self.timers,
                        &self.power,
                        &self.shelf,
                        &self.pages,
                    )?,
                    SurfaceKind::Dock => surface.paint_dock(
                        &self.dock_items,
                        &icons,
                        self.drag.as_ref(),
                        &self.pages.widget_readings,
                    )?,
                }
                surface.live = surface.motion.running(now)
                    || surface.volume_fill.running(now)
                    || surface.page_fx.running(now)
                    || surface.notice_fx.running(now)
                    || self.shelf.shots.running(now)
                    || surface.rail_fx.running(now)
                    || surface.slide.running(now)
                    || match &surface.role {
                        Role::Notch { intent, .. } => intent.is_some(),
                        Role::Dock { slots, .. } => {
                            slots.iter().any(|tween| tween.running(now))
                                || self.drag.as_ref().is_some_and(|drag| drag.gap.running(now))
                        }
                    };
                // A notch opened before a window covered it was held up; closed, it goes now.
                settled |= !surface.live
                    && surface.kind() == SurfaceKind::Notch
                    && !self.tuck_notch.shown(&surface.monitor);
            }
            if surface.leaving && !surface.slide.running(now) {
                surface.apply_visible(false);
            }
            if surface.overlay && !surface.live && surface.motion.value(now) <= 0.0 {
                surface.overlay = false;
                surface.set_visible(false);
            }
            running |= surface.live;
        }
        if docks_live
            && self
                .surfaces
                .iter()
                .any(|s| s.live && s.kind() == SurfaceKind::Dock)
        {
            self.animation_icons = Some(icons);
        }
        if settled {
            self.apply_visibility()?;
        }
        Ok(running)
    }
    fn close(&mut self, kind: SurfaceKind) {
        self.shown.retain(|shown| *shown != kind);
        self.surfaces.retain(|surface| surface.kind() != kind);
    }
    fn is_notch(&self, hwnd: HWND) -> bool {
        self.surfaces
            .iter()
            .any(|s| s.kind() == SurfaceKind::Notch && s.window.0 == hwnd)
    }
    fn dock_at(&self, hwnd: HWND) -> Option<&Surface> {
        self.surfaces
            .iter()
            .find(|s| s.kind() == SurfaceKind::Dock && s.window.0 == hwnd)
    }
    /// Starts a countdown the user asked for in the settings window; the text is the window's status line.
    /// Starts the current Pomodoro phase (or resumes a paused one) as a tagged timer.
    fn pomo_start(&mut self, now: Instant) -> String {
        let seconds = self
            .pages
            .pomo
            .left
            .take()
            .unwrap_or_else(|| self.pages.pomo.secs(&self.settings.pomo_cfg));
        let label = self.pages.pomo.phase.label();
        self.pages.pomo.track(&self.settings.pomo_cfg);
        if !self
            .timers
            .start_as(seconds, timer::wall_now(), now, Some(label))
        {
            self.pages.pomo.left = Some(seconds);
            return format!("{} timers are already running.", timer::MAX);
        }
        self.timers
            .write_to(&mut self.store, &mut self.pending, now);
        self.request_notches(Repaint::Full, |_| true);
        format!("{label}: {} min.", seconds.div_ceil(60))
    }
    /// Start or pause (false) or skip (true) from Settings; returns what to tell the user.
    fn pomo_act(&mut self, skip: bool) -> String {
        let now = Instant::now();
        let left = self.timers.stop_pomo(now);
        if skip {
            self.pages.pomo.advance(&self.settings.pomo_cfg);
        } else if left.is_some() {
            self.pages.pomo.left = left;
            self.timers
                .write_to(&mut self.store, &mut self.pending, now);
            self.request_notches(Repaint::Full, |_| true);
            return "Pomodoro paused.".to_owned();
        }
        self.pomo_start(now)
    }
    /// Pause (running) or resume (held) the plain countdown the Timer page shows.
    fn plain_act(&mut self, now: Instant) -> String {
        let paused = self
            .pages
            .timer_view(&self.timers, now)
            .is_some_and(|view| view.paused);
        if let (true, Some((left, total))) = (paused, self.pages.held) {
            if !self
                .timers
                .start_with(left, total, timer::wall_now(), now, None)
            {
                return format!("{} timers are already running.", timer::MAX);
            }
            self.pages.held = None;
            return "Timer resumed.".to_owned();
        }
        if self.pages.held.is_some() {
            return "One timer is already paused.".to_owned();
        }
        self.pages.held = self.timers.pause_plain(now);
        "Timer paused.".to_owned()
    }
    /// A Timer page button; what it did is announced as a notice (the presets just start, as before).
    fn timer_press(&mut self, control: timer::Ctl, now: Instant) {
        use timer::Ctl;
        let watching = self
            .pages
            .timer_view(&self.timers, now)
            .is_some_and(|view| view.watch);
        if watching || control == Ctl::Watch {
            match control {
                Ctl::Watch | Ctl::Resume => self.pages.stopwatch.start(now),
                Ctl::Pause => self.pages.stopwatch.pause(now),
                Ctl::Lap => self.pages.stopwatch.lap(now),
                Ctl::Reset => self.pages.stopwatch.reset(),
                _ => {}
            }
            return self.request_notches(Repaint::Full, |_| true);
        }
        let status = match control {
            Ctl::Preset(minutes) => {
                self.clear_notice();
                return drop(self.start_timer(minutes * 60));
            }
            Ctl::Pause | Ctl::Resume
                if self
                    .pages
                    .timer_view(&self.timers, now)
                    .is_some_and(|view| !view.tagged) =>
            {
                self.plain_act(now)
            }
            Ctl::Watch | Ctl::Lap | Ctl::Reset => return, // handled above
            Ctl::Focus | Ctl::Pause | Ctl::Resume => self.pomo_act(false),
            Ctl::Skip => self.pomo_act(true),
            Ctl::Cancel => {
                let view = self.pages.timer_view(&self.timers, now);
                if view.as_ref().is_some_and(|view| view.tagged) {
                    self.timers.stop_pomo(now);
                    self.pages.pomo = pomo::Pomo::default();
                    "Pomodoro cancelled.".to_owned()
                } else if view.is_some_and(|view| view.paused) {
                    self.pages.held = None;
                    "Timer cancelled.".to_owned()
                } else {
                    self.timers.cancel_soonest();
                    "Timer cancelled.".to_owned()
                }
            }
        };
        self.timers
            .write_to(&mut self.store, &mut self.pending, now);
        // A phase starting shows in the page's title line, so it needs no notice of its own, and an earlier
        // notice ("Pomodoro paused.") is stale by then.
        if pomo::is_start_status(&status) {
            self.clear_notice();
        } else {
            self.show_notice(status, now);
        }
    }
    /// Ends a plain notice at once when what it said no longer holds; a question or button row stays.
    fn clear_notice(&mut self) {
        if self.power.clear_plain() {
            self.request_notches(Repaint::Full, |_| true);
        }
    }
    fn start_timer(&mut self, seconds: u64) -> String {
        let now = Instant::now();
        if !self.timers.start(seconds, timer::wall_now(), now) {
            return format!("{} timers are already running.", timer::MAX);
        }
        self.timers
            .write_to(&mut self.store, &mut self.pending, now);
        self.request_notches(Repaint::Full, |_| true);
        "Timer started.".to_owned()
    }
    fn effective_accent(&self) -> Option<crate::theme::Rgb> {
        if self.settings.match_accent {
            crate::theme::windows_accent_color().or_else(|| Some(self.text.accent()))
        } else {
            None
        }
    }
}

impl Desktop {
    /// Moves, resizes and repaints every dock for the current style, buttons and slot count. Same slots in a
    /// new place or width: no new surface and no timer; hover restarts (cells moved).
    fn relayout_docks(&mut self) -> Result<()> {
        let (style, buttons, slots) = (
            self.settings.dock_style,
            self.dock_buttons(),
            self.dock_slots(),
        );
        let icons = self.dock_icons();
        self.animation_icons = None;
        for dock in &mut self.surfaces {
            let Some(monitor) = self.monitors.iter().find(|m| m.device == dock.monitor) else {
                continue;
            };
            if dock.kind() != SurfaceKind::Dock {
                continue;
            }
            let Role::Dock {
                hover,
                slots: tweens,
                ..
            } = &mut dock.role
            else {
                continue;
            };
            let buttons = buttons.fit_widgets(monitor.dock_room(style).width, dock.dpi);
            (*hover, *tweens) = (
                None,
                vec![Tween::rest(0.0); dock::cell_count(buttons, slots)],
            );
            (dock.dock_style, dock.dock_buttons) = (style, buttons);
            dock.bounds = placement_styled(
                monitor.dock_room(style),
                dock.dpi,
                SurfaceKind::Dock,
                slots,
                style,
                buttons,
            );
            dock.window.resize(dock.bounds)?;
            dock.canvas.resize(dock.bounds)?;
            dock.paint_dock(
                &self.dock_items,
                &icons,
                self.drag.as_ref(),
                &self.pages.widget_readings,
            )?;
        }
        Ok(())
    }
    /// Rebuilds dock items only when the catalog generation or an icon changed.
    /// A shell hook event: `code` is the HSHELL value, `window` the handle. A flash marks the window; activating
    /// or closing it clears the mark. The dock is rebuilt on the next pass when the marks changed.
    fn note_shell_event(&mut self, code: usize, window: usize) {
        const DESTROYED: usize = 2;
        const ACTIVATED: usize = 4;
        const FLASH: usize = 0x8006;
        const RUDE_ACTIVATED: usize = 0x8004;
        // Undocumented, seen on Windows 11: sent for a window covering its monitor, on any monitor.
        const FULLSCREEN_ENTER: usize = 0x35;
        const FULLSCREEN_EXIT: usize = 0x36;
        if matches!(code, FULLSCREEN_ENTER | FULLSCREEN_EXIT | DESTROYED)
            && native_window::note_fullscreen(window, code == FULLSCREEN_ENTER)
        {
            self.fullscreen_generation = None; // the next pass admits or drops it
        }
        let marked = self.flashing.contains(&window);
        match code {
            FLASH if !marked => self.flashing.push(window),
            DESTROYED | ACTIVATED | RUDE_ACTIVATED if marked => {
                self.flashing.retain(|w| *w != window)
            }
            _ => return,
        }
        self.dock_generation = None;
    }
    fn refresh_dock(&mut self, model: &WindowModel) -> Result<()> {
        let icons_arrived = self.icons.collect();
        self.icons_arrived = icons_arrived;
        if icons_arrived {
            self.animation_icons = None;
        }
        if icons_arrived {
            self.fill_stack_icons();
        }
        if self.dock_generation == Some(model.generation) && !icons_arrived {
            return Ok(());
        }
        if self.dock_generation != Some(model.generation) {
            // A pin follows the identity its app's windows now group under, from here on.
            for (pin, to) in crate::window_model::pin_upgrades(&model.windows) {
                if self.pins.position(&pin).is_some() || self.pins.hides(&pin, None) {
                    self.change_pins(|pins| pins.upgrade(&pin, &to));
                }
            }
            self.dock_generation = Some(model.generation);
            let mut items = dock::items_with_pins(model, &self.pins, self.settings.desktop_only);
            dock::with_recent(
                &mut items,
                &mut self.recent,
                &self.dock_items,
                &self.pins,
                model,
            );
            for item in &mut items {
                if !self.settings.dock_badges {
                    item.badge = dock::Badge::None;
                } else if !item.active
                    && item
                        .windows
                        .iter()
                        .any(|key| self.flashing.contains(&key.handle))
                {
                    item.badge = item.badge.max(dock::Badge::Dot);
                }
            }
            let changed = items != self.dock_items;
            // Only changes to visible dock data require a repaint.
            if !changed && !icons_arrived {
                return Ok(());
            }
            if changed {
                // A slider popup is about the system, not a dock slot: a changing app list leaves it open.
                if self
                    .hover_popup
                    .as_ref()
                    .is_some_and(|popup| matches!(popup.hover_hit, Some(Hit::Slot(_))))
                {
                    self.hover_popup = None;
                }
                let shelf = self.shelf.shelf.entries();
                self.icons.retain(|identity| {
                    items.iter().any(|item| &item.identity == identity || item.members.iter().any(|member| &member.identity == identity))
                        || matches!(identity, Identity::Executable(path) if shelf.iter().any(|e| &e.path == path))
                        || matches!(identity, Identity::Executable(path) if self.stack.iter().any(|file| file.to_string_lossy() == *path))
                });
            }
            let resized = items.len() != self.dock_items.len();
            self.dock_items = items;
            self.animation_icons = None;
            if resized {
                return self.relayout_docks();
            }
        }
        let icons = self.dock_icons();
        for dock in self
            .surfaces
            .iter_mut()
            .filter(|s| s.kind() == SurfaceKind::Dock && s.visible)
        {
            dock.paint_dock(
                &self.dock_items,
                &icons,
                self.drag.as_ref(),
                &self.pages.widget_readings,
            )?;
        }
        Ok(())
    }
    /// Album art side to decode, in physical pixels: the panel cover on the densest notch (also while
    /// it is hidden), or 0 with no notch surface, which skips the decode until one exists.
    // shortcut: art keeps its decode size after a DPI or text-size change until the track changes; re-decode if it looks soft.
    fn cover_pixels(&self) -> u32 {
        self.surfaces
            .iter()
            .filter(|s| s.kind() == SurfaceKind::Notch)
            .map(|s| (COVER_DIPS * s.dpi as f32 / 96.0).round() as u32)
            .max()
            .unwrap_or(0)
    }
    /// The one icon size the worker's cache holds: the densest dock's. The shelf shares it (a different size
    /// would empty the cache on every switch) and draws it smaller; a notch-only desktop uses the notch's DPI.
    fn icon_size(&self) -> Option<u32> {
        let densest = |kind| {
            self.surfaces
                .iter()
                .filter(|s| s.kind() == kind)
                .map(|s| s.dpi)
                .max()
        };
        icon_dpi(densest(SurfaceKind::Dock), densest(SurfaceKind::Notch))
            .map(|dpi| dock::px(dock::ICON_DIPS, dpi) as u32)
    }
    /// One icon set for every dock, extracted for the densest dock; lower-DPI docks scale it down.
    // shortcut: mixed-DPI docks resample icons; keep one cache size per DPI if that looks soft on hardware.
    fn dock_icons(&mut self) -> Vec<Vec<Option<Arc<IconPixels>>>> {
        let Some(size) = self.icon_size() else {
            return Vec::new();
        };
        self.dock_items
            .iter()
            .map(|item| {
                let members = if item.members.is_empty() {
                    std::slice::from_ref(item)
                } else {
                    &item.members
                };
                members
                    .iter()
                    .map(|member| {
                        let sources =
                            icons::sources(&member.identity, member.executable.as_deref());
                        self.icons.icon(&member.identity, sources, size)
                    })
                    .collect()
            })
            .collect()
    }
    /// A slot acts on its real window group, a button does its job.
    fn activate(&mut self, hwnd: HWND, hit: Option<Hit>) -> Result<()> {
        // A newer click supersedes a Places or stack answer still out. The clicks that ask do not invalidate: a refused
        // one (still reading) must leave the first answer standing, and an accepted one supersedes it in `places::start`.
        match hit {
            Some(Hit::Slot(slot)) => {
                if let Some(item) = self.dock_items.get(slot)
                    && !item.members.is_empty()
                {
                    crate::places::invalidate();
                    self.pending_popup = None;
                    let members = item.members.clone();
                    for member in &members {
                        self.open_member(member);
                    }
                    return Ok(());
                }
                // A stack rises from a dock, so a shortcut key (no dock window) opens a folder like any pin. Whether a path is
                // a folder is a worker's to say (`stack_ready` goes on from there): a slow share must not stop the host.
                if !hwnd.is_invalid()
                    && let Some(item) = self.dock_items.get(slot)
                    && item.windows.is_empty()
                    && let Identity::Executable(path) = &item.identity
                    && crate::places::may_be_folder(path)
                {
                    self.ask_stack(hwnd, slot, path.clone());
                    return Ok(());
                }
                crate::places::invalidate();
                self.pending_popup = None;
                self.run_pin(slot, None);
                Ok(())
            }
            Some(Hit::Button(button)) => {
                self.press_button(hwnd, button);
                Ok(())
            }
            None => Ok(()),
        }
    }
    fn run_dock_action(&mut self, action: dock::Action) {
        if let Some(failure) = dock::action_failure(action) {
            self.show_notice(failure.into(), Instant::now());
        }
    }
    fn open_member(&mut self, member: &DockItem) {
        if let Some(action) = dock::open_action(member) {
            self.run_dock_action(action);
        }
    }

    fn refresh_hover_popup(&mut self) -> u32 {
        let now = Instant::now();
        if self.menu.is_some() || self.drag.as_ref().is_some_and(|drag| drag.active) {
            self.hover_popup = None;
            self.hover_spot = None;
            return INFINITE;
        }
        let slots = self.dock_slots();
        let hovered = self.surfaces.iter().find_map(|surface| {
            if let Role::Dock {
                hover: Some(cell), ..
            } = surface.role
                && surface.visible
            {
                dock::cell(surface.dock_buttons, slots, cell).map(|hit| (surface.window.0, hit))
            } else {
                None
            }
        });
        if self.hover_spot.as_ref().map(|&(hwnd, hit, _)| (hwnd, hit))
            != hovered.map(|(hwnd, hit)| (hwnd.0 as usize, hit))
        {
            self.hover_spot =
                hovered.map(|(hwnd, hit)| (hwnd.0 as usize, hit, now + preview::DELAY));
        }
        if let Some(popup) = &mut self.hover_popup {
            let same = hovered
                .is_some_and(|(hwnd, hit)| hwnd == popup.dock() && Some(hit) == popup.hover_hit);
            let owner_visible = self
                .surfaces
                .iter()
                .any(|surface| surface.window.0 == popup.dock() && surface.visible);
            if !owner_visible {
                self.hover_popup = None;
                return INFINITE;
            }
            if same || popup.under_pointer() {
                popup.close_at = None;
                if popup.closing() {
                    popup.set_open(true);
                }
            } else if !popup.closing() {
                let due = *popup
                    .close_at
                    .get_or_insert(now + Duration::from_millis(180));
                if now >= due {
                    popup.close_at = None;
                    popup.set_open(false);
                }
            }
            if popup.closing() && !popup.running(now) {
                self.hover_popup = None;
            }
        }
        if self.hover_popup.is_none()
            && let Some((hwnd, hit, due)) = self.hover_spot
            && now >= due
        {
            self.show_hover_popup(HWND(hwnd as *mut _), hit);
        }
        if let Some(popup) = &mut self.hover_popup {
            if popup.paint().is_err() {
                self.hover_popup = None;
                return INFINITE;
            }
            return popup
                .close_at
                .map_or(INFINITE, |due| milliseconds_until(due, now));
        }
        self.hover_spot
            .filter(|(_, _, due)| *due > now)
            .map_or(INFINITE, |(_, _, due)| milliseconds_until(due, now))
    }

    fn show_hover_popup(&mut self, hwnd: HWND, hit: Hit) {
        if hit == Hit::Button(Button::Quick) {
            return self.show_command_center(hwnd);
        }
        if let Hit::Button(button) = hit
            && button.widget().is_some()
        {
            return self.show_widget_popup(hwnd, button);
        }
        if hit == Hit::Button(Button::Limits) {
            return self.show_limits_popup(hwnd);
        }
        if hit == Hit::Button(Button::Moon) {
            return self.show_moon_popup(hwnd);
        }
        if let Hit::Button(button @ (Button::Volume | Button::Brightness)) = hit {
            return self.show_levels_popup(hwnd, button);
        }
        // Only a folder's member list and the AI limits pop up; the dock has no tooltips.
        let Some(item) = hit
            .slot()
            .and_then(|slot| self.dock_items.get(slot))
            .filter(|item| !item.members.is_empty())
        else {
            return;
        };
        let (identity, members) = (item.identity.clone(), item.members.clone());
        let label = dock::pinned_as(item, &self.pins)
            .as_ref()
            .and_then(|pin| self.pins.folder(pin))
            .map_or_else(
                || crate::pins::display_name(&item.identity),
                |folder| folder.name.clone(),
            );
        let Some(dock) = self.dock_at(hwnd) else {
            return;
        };
        let Some(work) = self
            .monitors
            .iter()
            .find(|monitor| monitor.device == dock.monitor)
            .map(|monitor| monitor.work)
        else {
            return;
        };
        let (left, width) = dock::span(dock.dock_buttons, dock.bounds.width, dock.dpi, hit);
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: hit.slot().unwrap_or(dock_menu::APP),
            identity,
            windows: Vec::new(),
            items: std::iter::once(Item::Label)
                .chain((0..members.len()).map(Item::Member))
                .collect(),
            dock_bounds: dock.bounds,
            work,
            dpi: dock.dpi,
            app: label.clone(),
            titles: std::iter::once(label)
                .chain(
                    members
                        .iter()
                        .map(|member| crate::pins::display_name(&member.identity)),
                )
                .collect(),
            icons: Vec::new(),
            x: dock.bounds.x + left + width / 2,
            below: None,
            return_to: None,
        };
        let size = self.icon_size().unwrap_or(40);
        let mut spec = spec;
        spec.icons = std::iter::once(None)
            .chain(members.iter().map(|member| {
                self.icons.icon(
                    &member.identity,
                    icons::sources(&member.identity, member.executable.as_deref()),
                    size,
                )
            }))
            .collect();
        match dock_menu::Menu::open_hover(&self.graphics, self.palette, spec) {
            Ok(mut popup) => {
                popup.hover_hit = Some(hit);
                self.hover_popup = Some(popup);
            }
            Err(error) => eprintln!("Dock folder: {error}"),
        }
    }

    /// The AI limits over their dock button: the last readings at once, and a read of what changed or aged out.
    fn show_limits_popup(&mut self, hwnd: HWND) {
        let Some((buttons, bounds, dpi, work)) = self.dock_at(hwnd).and_then(|dock| {
            self.monitors
                .iter()
                .find(|monitor| monitor.device == dock.monitor)
                .map(|monitor| (dock.dock_buttons, dock.bounds, dock.dpi, monitor.work))
        }) else {
            return;
        };
        let now = limits_os::now();
        self.limits.configure(
            (self.settings.limits_codex, self.settings.limits_agy),
            self.settings.limits_group,
        );
        self.start_limits(now, false);
        self.limits_open = true;
        self.pages.limits = limits::view(
            &self.limits.rows,
            now,
            &limits::clock(prayer_os::local_now().offset_min),
            self.limits.busy(),
        );
        self.pages.limits_key = Some((self.limits.rev, self.limits.busy(), self.clock.clone()));
        let (items, titles) = self.limits_items();
        let hit = Hit::Button(Button::Limits);
        let (left, width) = dock::span(buttons, bounds.width, dpi, hit);
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: dock_menu::APP,
            identity: Identity::Unavailable(0),
            windows: Vec::new(),
            items,
            dock_bounds: bounds,
            work,
            dpi,
            app: "AI limits".into(),
            titles,
            icons: Vec::new(),
            x: bounds.x + left + width / 2,
            below: None,
            return_to: None,
        };
        match dock_menu::Menu::open_hover(&self.graphics, self.palette, spec) {
            Ok(mut popup) => {
                popup.hover_hit = Some(hit);
                self.hover_popup = Some(popup);
            }
            Err(error) => eprintln!("Dock limits: {error}"),
        }
    }

    /// A global shortcut (`hotkeys.rs`) fired; a message queued just before its switch went off is ignored.
    fn hotkey(&mut self, id: usize) -> Result<()> {
        let Some((group, index)) =
            hotkeys::action(id).filter(|&(group, _)| self.settings.hotkeys[group])
        else {
            return Ok(());
        };
        match group {
            0 => self.toggle_notch(),
            1 => self.activate(HWND::default(), Some(Hit::Slot(index)))?,
            2 => {
                self.dock_away = !self.dock_away;
                self.apply_visibility()?;
            }
            4 => self.toggle_mic(),
            _ => {
                let dock = self
                    .surfaces
                    .iter()
                    .find(|s| s.kind() == SurfaceKind::Dock && s.dock_buttons.on(Button::Tray))
                    .map(|s| s.window.0);
                if let Some(hwnd) = dock {
                    // The keys are still down: lift Ctrl and Alt so the flyout's Win+B is not Ctrl+Alt+Win+B.
                    dock::release_keys(&[VK_CONTROL, VK_MENU]);
                    self.open_flyout(hwnd, crate::anchored::Kind::Tray);
                } else {
                    self.show_notice("Turn on the hidden icons button.".into(), Instant::now());
                }
            }
        }
        Ok(())
    }
    /// Opens every closed notch, or closes the open ones.
    fn toggle_notch(&mut self) {
        let open = self
            .surfaces
            .iter()
            .any(|s| s.kind() == SurfaceKind::Notch && s.motion.target() >= 1.0);
        if !open {
            return self.open_notches();
        }
        for surface in &mut self.surfaces {
            if let Role::Notch { intent, linger, .. } = &mut surface.role {
                *intent = None;
                linger.clear();
                surface
                    .motion
                    .retarget(0.0, motion::CLOSE_TIME, motion::CLOSE);
                surface.live = true;
            }
        }
    }
    fn change_pins(&mut self, edit: impl FnOnce(&mut Pins) -> bool) {
        let now = Instant::now();
        if edit_pins(
            &mut self.pins,
            &mut self.store,
            &mut self.pending,
            now,
            edit,
        ) {
            self.hover_popup = None;
            self.dock_generation = None;
            // Every pin change (dock click, drag, settings move) refreshes the list so a later Move is not stale.
            let selected = self
                .settings_window
                .as_ref()
                .map_or(0, SettingsWindow::pin_selection);
            self.sync_settings_pins(selected);
        }
    }
    /// Left button released on a dock. Returns true when it ended a reorder drag (so it is not a click):
    /// the pin moves to the slot under the pointer, as one store change.
    fn end_drag(&mut self, hwnd: HWND, lparam: LPARAM) -> bool {
        let Some(drag) = take_drag(&mut self.drag) else {
            return false;
        };
        if !drag.active {
            return false;
        }
        let slots = self.dock_slots();
        // The lifted icon and the gap go (a drop that changes nothing still needs this repaint).
        if let Some(dock) = self.surfaces.iter_mut().find(|s| s.window.0 == hwnd) {
            dock.live = true;
        }
        let drop = self
            .dock_at(hwnd)
            .and_then(|dock| dock.drop_at(point(lparam).0, self.dock_items.len(), slots));
        if let Some((slot, group)) = drop
            && let (Some(item), Some(source)) =
                (self.dock_items.get(slot), self.dock_items.get(drag.from))
        {
            let target = dock::pinned_as(item, &self.pins).unwrap_or_else(|| item.identity.clone());
            let group = group && source.members.is_empty();
            let source =
                dock::pinned_as(source, &self.pins).unwrap_or_else(|| source.identity.clone());
            let mut full = false;
            self.change_pins(|pins| {
                if group {
                    return pins.group(&source, &target);
                }
                // A running app that is not pinned pins where it is dropped (never on its own slot).
                let added = pins.position(&source).is_none();
                if added && (slot == drag.from || !menus::add_pin(pins, source.clone(), &mut full))
                {
                    return false;
                }
                let moved = pins
                    .position(&source)
                    .zip(
                        pins.position(&target)
                            .or_else(|| pins.items().len().checked_sub(1)),
                    )
                    .is_some_and(|(from, to)| pins.move_to(from, to));
                added || moved
            });
            self.say_pins_full(full);
        }
        true
    }
    /// Applies queued drops without extracting file artwork or polling the file system.
    #[inline(never)]
    fn refresh_shelf(&mut self) {
        let events = std::mem::take(&mut *self.drops.borrow_mut());
        for event in events {
            match event {
                DropEvent::Hover(hwnd, over) => {
                    let shelf = self.pages.tabs.on(Tab::Shelf);
                    if let Some(notch) = self.surfaces.iter_mut().find(|s| s.window.0 == hwnd) {
                        notch.set_notch_hover(over);
                        // A file over the notch is for the shelf: open it there (a disabled Shelf tab keeps
                        // the notch as it was).
                        if over
                            && shelf
                            && notch.visible
                            && let Role::Notch { intent, .. } = &mut notch.role
                        {
                            open_notch(intent, &mut notch.motion);
                            notch.live = true;
                        }
                    }
                    if over && shelf {
                        self.select_tab(Tab::Shelf);
                    }
                }
                DropEvent::Files(paths) if self.pages.tabs.on(Tab::Shelf) => {
                    self.add_shelf_files(&paths)
                }
                DropEvent::Files(_) => {}
                DropEvent::Shot(_) => {}
                DropEvent::Dock(hwnd, x, paths) => self.drop_on_dock(hwnd, x, &paths),
                DropEvent::DockOver(hwnd, x) => self.drag_over_dock(hwnd, x),
            }
        }
    }
    /// A file drag rested on a running app's slot: its window comes forward, once per rest. A group of apps
    /// and an app with no window are left alone. The wait is the one deadline, none while nothing is armed.
    fn refresh_raise(&mut self) -> u32 {
        let now = Instant::now();
        if let Some((_, slot)) = self.raise.take_due(now)
            && let Some(item) = self.dock_items.get(slot)
            && item.members.is_empty()
            && let Some(&key) = item.windows.first()
        {
            let _ = dock::perform(dock::Action::Activate(key));
        }
        self.raise
            .deadline()
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// Opens the picker; a finished one whose completion wake was lost is collected instead of stranding it.
    fn start_file_picker(&mut self) {
        if let Some(paths) = self.file_picker.start() {
            self.add_shelf_files(&paths);
        }
    }
    fn add_shelf_files(&mut self, paths: &[String]) {
        self.shelf.copied = false; // dropped files show on the Files list
        let before = self.shelf.shelf.entries().len();
        let mut displaced = 0;
        self.change_shelf(|shelf| {
            let added = shelf.add_all(paths);
            displaced = before + added - shelf.entries().len();
            added > 0
        });
        if displaced > 0 {
            self.show_notice(
                format!("Shelf full. Removed {displaced} older."),
                Instant::now(),
            );
        }
    }

    fn change_shelf(&mut self, edit: impl FnOnce(&mut Shelf) -> bool) {
        let now = Instant::now();
        if edit_shelf(
            &mut self.shelf,
            &mut self.store,
            &mut self.pending,
            now,
            edit,
        ) {
            self.request_notches(Repaint::Full, |_| true);
        }
    }
    /// The visible shelf entry under a window point of notch `notch`, if its slot is drawn and filled.
    fn shelf_slot_at(&self, notch: usize, lparam: LPARAM) -> Option<usize> {
        let surface = &self.surfaces[notch];
        let (x, y) = point(lparam);
        let progress = surface.motion.value(Instant::now());
        let media = self.pages.track(&self.media).is_some();
        if self.page() != Tab::Shelf {
            return None;
        }
        layout::shelf_at(
            surface.bounds.width,
            surface.dpi,
            progress,
            media,
            self.shelf.shelf.is_empty(),
            x,
            y,
        )
        .filter(|&index| {
            self.shelf.shelf.absolute(index).is_some()
                || (index == shelf::VISIBLE - 1 && self.shelf.shelf.overflow())
                || (index == 0 && self.shelf.shelf.is_empty())
        })
    }
    /// A user activation of shelf entry `index`: open it, or with `reveal` select it in Explorer. The only
    /// existence check in the shelf's life happens here, never for a UNC path (`shelf_os::open` runs that
    /// call off the host thread); a target that is gone is marked, not opened, and not removed.
    fn open_shelf(&mut self, index: usize, reveal: bool) {
        if index >= self.shelf.shelf.slot_count() {
            return;
        }
        if self.shelf.shelf.is_empty() && index == 0 {
            self.start_file_picker();
            return;
        }
        let Some(index) = self.shelf.shelf.absolute(index) else {
            self.change_shelf(Shelf::next_page);
            return;
        };
        let Some(entry) = self
            .shelf
            .shelf
            .entries()
            .get(index)
            .filter(|e| e.shot.is_none())
        else {
            return;
        };
        let path = entry.path.clone();
        // A UNC path or a mapped drive could stall the host on a dead share, so neither is stat'ed here.
        let network = shelf_os::is_network(&path);
        let found = network || std::path::Path::new(&path).exists();
        if self.shelf.shelf.set_missing(index, !found) {
            self.request_notches(Repaint::Full, |_| true);
        }
        if found && !shelf_os::open(&path, reveal, network) {
            self.show_open_failed();
        } else if !found {
            self.show_notice(
                "File missing. Add it from its new location.".into(),
                Instant::now(),
            );
        }
    }
    /// Opens the Shelf context menu for a drawn entry.
    fn shelf_menu_at(&mut self, hwnd: HWND, lparam: LPARAM) -> bool {
        if self.shelf.copied {
            return false;
        }
        let notch = self
            .surfaces
            .iter()
            .position(|s| s.kind() == SurfaceKind::Notch && s.window.0 == hwnd);
        let Some(index) = notch.and_then(|notch| self.shelf_slot_at(notch, lparam)) else {
            return false;
        };
        let Some(entry) = self
            .shelf
            .shelf
            .absolute(index)
            .and_then(|index| self.shelf.shelf.entries().get(index))
            .filter(|entry| entry.shot.is_none())
        else {
            return false;
        };
        let path = entry.path.clone();
        self.open_context_menu(hwnd, lparam, Some(path));
        true
    }

    fn remove_shelf_entry(&mut self, path: &str) {
        let Some(index) = self
            .shelf
            .shelf
            .entries()
            .iter()
            .position(|entry| entry.path == path)
        else {
            return;
        };
        self.shelf.removed = Some((index, self.shelf.shelf.entries()[index].clone()));
        self.change_shelf(|shelf| shelf.remove(index));
        self.show_notice(
            "Removed from Shelf. Select Undo to restore.".into(),
            Instant::now(),
        );
    }

    /// The clipboard changed: keep its text on the Copied list when capture is on. Contents are never logged.
    fn capture_clip(&mut self) {
        let hwnd = self
            .surfaces
            .iter()
            .find(|s| s.kind() == SurfaceKind::Notch)
            .map(|s| s.window.0);
        if self.settings.clipboard
            && let Some(hwnd) = hwnd
        {
            match clipboard_os::read(hwnd) {
                clipboard_os::Read::Text(text) => self.change_clips(|clips| clips.add(&text)),
                clipboard_os::Read::Nothing => {}
                clipboard_os::Read::Busy => native_window::clip_again(),
            }
        }
    }
    /// One edit of the Copied list (memory only, never written to disk): a real change repaints an open panel.
    fn change_clips(&mut self, edit: impl FnOnce(&mut Clips) -> bool) {
        if edit(&mut self.shelf.clips) {
            self.request_notches(Repaint::Full, Surface::expanded);
        }
    }
    /// Files | Copied: which list the Shelf page shows.
    fn set_copied(&mut self, copied: bool) {
        if self.shelf.copied != copied {
            self.shelf.copied = copied;
            self.request_notches(Repaint::Full, Surface::expanded);
        }
    }
    /// A click on the Copied list: Clear, a remove cross, "More" or a clip, which goes back on the clipboard.
    fn click_clips(&mut self, notch: usize, (x, y): (i32, i32)) -> bool {
        let surface = &self.surfaces[notch];
        let (width, dpi, hwnd) = (surface.bounds.width, surface.dpi, surface.window.0);
        let progress = surface.motion.value(Instant::now());
        let live = self.pages.track(&self.media).is_some();
        let clips = &self.shelf.clips;
        if surface.hovered
            && !clips.is_empty()
            && layout::shelf_action_at(width, dpi, progress, live, (x, y), false)
        {
            self.change_clips(Clips::clear);
            return true;
        }
        if let Some(at) = layout::shelf_remove_at(width, dpi, progress, false, x, y)
            .and_then(|slot| clips.absolute(slot))
        {
            self.change_clips(|clips| clips.remove(at));
            return true;
        }
        let Some(slot) =
            layout::shelf_at(width, dpi, progress, live, false, x, y).filter(|&slot| {
                slot < clips.slot_count()
                    && (clips.absolute(slot).is_some()
                        || (slot == clipboard::VISIBLE - 1 && clips.has_more()))
            })
        else {
            return false;
        };
        match clips.absolute(slot) {
            Some(at) => {
                if !clipboard_os::write(hwnd, &clips.items()[at]) {
                    self.show_notice("Couldn't copy.".into(), Instant::now());
                }
            }
            None => self.change_clips(|clips| {
                clips.next_page();
                true
            }),
        }
        true
    }
    fn undo_shelf_removal(&mut self) {
        if let Some((index, entry)) = self.shelf.removed.take() {
            let mut restored = false;
            self.change_shelf(|shelf| {
                restored = shelf.restore(index, entry.clone());
                restored
            });
            if restored {
                self.show_notice("Restored to Shelf.".into(), Instant::now());
            } else {
                self.shelf.removed = Some((index, entry));
                self.show_notice(
                    "Can't restore. Check Shelf space and duplicates.".into(),
                    Instant::now(),
                );
            }
        }
    }

    fn command(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::Quit => return Ok(false),
            Command::ShowSettings | Command::Agent => {}
            Command::ShowNotch => self.show(SurfaceKind::Notch)?,
            Command::ShowDock => self.show(SurfaceKind::Dock)?,
            Command::HideNotch | Command::HideDock => {
                let kind = if command == Command::HideNotch {
                    SurfaceKind::Notch
                } else {
                    SurfaceKind::Dock
                };
                // The last surface stays: with none shown the host would end.
                if self.shown.iter().any(|shown| *shown != kind) {
                    self.close(kind);
                }
            }
        }
        if matches!(command, Command::ShowNotch | Command::ShowDock) {
            // Also recovers monitors whose earlier placement ran out of retries.
            self.request_topology();
            // A dock or notch auto-hidden behind a window (or put away by its hotkey or the tray) comes back for a while.
            self.hidden = false;
            let now = Instant::now();
            if command == Command::ShowDock {
                self.dock_away = false;
                self.tuck.reveal(now, SHOW_COMMAND);
            } else {
                self.tuck_notch.reveal(now, SHOW_COMMAND);
            }
            self.apply_visibility()?;
        }
        Ok(!self.shown.is_empty())
    }
    fn message(&mut self, message: &MSG) -> Result<()> {
        // These posts only wake the loop: `read_wakes` reads their flags on every pass.
        if matches!(
            message.message,
            native_window::CLIPBOARD
                | native_window::POWER_STATUS
                | native_window::DEVICE
                | native_window::WAKE
        ) {
            return Ok(());
        }
        if message.message == native_window::REPOSITION {
            if matches!(
                message.lParam.0 as u32,
                WM_SETTINGCHANGE | WM_SYSCOLORCHANGE
            ) {
                self.palette_dirty = true;
            }
            // Every surface forwards display/DPI/settings/time changes, resume and taskbar recreation;
            // one coalesced read serves them all.
            self.request_topology();
            crate::taskbar::hide_again(); // a restarted Explorer or a new monitor shows a new taskbar
            if message.wParam.0 != 0
                && let Some(tray) = &self.tray
            {
                tray.show(); // a new taskbar (or a resume) lost or kept the icon; adding again is safe
            }
            // A clock, time-zone or locale change, or a resume, can alter the text before the deadline.
            self.clock_due = Instant::now();
            self.prayer.key = None; // the day's times are computed again
            self.water_due = None;
            self.pages.prayer_key = None;
            // Countdowns keep their remaining time; only their stored wall-clock expiry moves.
            let now = Instant::now();
            if self.timers.resync(timer::wall_now(), now) {
                self.timers
                    .write_to(&mut self.store, &mut self.pending, now);
            }
            if message.wParam.0 != 0 {
                // Resume or a new taskbar: re-read what events may have missed. The bits coalesce, so
                // the post from every surface costs one re-read.
                self.media.reconnect();
                self.audio.reconnect();
                if let Some(eyes) = &mut self.pages.eyes {
                    eyes.wake(now);
                    self.request_notches(Repaint::Full, |s| !s.live && s.motion.value(now) < 0.1);
                }
            }
            return Ok(());
        }
        // The volume row answers the pointer only on the Media page with a live track.
        let volume_ui = self.page() == Tab::Media && self.pages.track(&self.media).is_some();
        let slots = self.dock_slots();
        let Some(surface) = self
            .surfaces
            .iter_mut()
            .find(|surface| surface.window.0 == message.hwnd)
        else {
            return Ok(());
        };
        match message.message {
            native_window::CLOSE_SURFACE => {
                let kind = surface.kind();
                self.close(kind);
            }
            WM_LBUTTONDOWN if matches!(surface.role, Role::Dock { .. }) => {
                // Only a pinned slot can be dragged; pins are always the first items.
                let from = surface
                    .dock_hit(message.lParam, slots)
                    .and_then(Hit::slot)
                    .filter(|&index| {
                        self.dock_items.get(index).is_some_and(|item| {
                            crate::pins::launch_target(&item.identity).is_some()
                        })
                    });
                self.drag = from.map(|from| Drag {
                    from,
                    x: point(message.lParam).0,
                    active: false,
                    at: 0,
                    gap: Tween::rest(from as f32),
                    group: None,
                });
                if self.drag.is_some() {
                    // SAFETY: captures the pointer for this thread's own live dock window.
                    unsafe {
                        SetCapture(surface.window.0);
                    }
                }
            }
            WM_MOUSEMOVE => {
                let entered = surface.track_pointer()?;
                if let Some(drag) = &mut self.drag {
                    if message.wParam.0 & MK_LBUTTON == 0 {
                        // The release was lost: no drag, no click. ROQA is never the foreground app, so a
                        // capture it still holds would take every pointer move over its windows (the edge
                        // strips included) and nothing would reveal.
                        take_drag(&mut self.drag);
                        surface.live = true;
                    } else {
                        let x = point(message.lParam).0;
                        drag.active |= dock::drag_started(drag.x, x, surface.dpi);
                        if drag.active {
                            drag.at = x;
                            let drop = surface.drop_at(x, self.dock_items.len(), slots);
                            drag.group = drop
                                .filter(|&(slot, middle)| middle && slot != drag.from)
                                .map(|(slot, _)| slot);
                            if let (None, Some((slot, _))) = (drag.group, drop) {
                                drag.gap
                                    .retarget(slot as f32, motion::POP_TIME, motion::POP);
                            }
                            surface.live = true;
                        }
                    }
                }
                if let Role::Dock { hover, .. } = surface.role {
                    // While dragging, only the folder target lights up.
                    let cell = match &self.drag {
                        Some(drag) if drag.active => drag.group.and_then(|slot| {
                            dock::cell_index(surface.dock_buttons, slots, Hit::Slot(slot))
                        }),
                        _ => surface.dock_cell(message.lParam, slots),
                    };
                    if entered || cell != hover {
                        surface.hover_dock(cell, true);
                    }
                } else {
                    let (x, y) = point(message.lParam);
                    let now = Instant::now();
                    let progress = surface.motion.value(now);
                    let width = surface.bounds.width;
                    let media = self.pages.track(&self.media).is_some();
                    let rail = surface.tab_rail(&self.pages, media, now);
                    let over = layout::notch_hit(width, surface.dpi, progress, media, rail, x, y);
                    // While a menu is open the notch does not open under it.
                    let was = matches!(surface.role, Role::Notch { engaged: true, .. });
                    surface.notice_hover(over && self.menu.is_none(), media, message);
                    if over
                        && !was
                        && progress < 0.1
                        && let Some(eyes) = &mut self.pages.eyes
                        && eyes.poke(now)
                    {
                        surface.request(Repaint::Full);
                    }
                    // The tab cell under the pointer washes lighter.
                    let tab = rail.and_then(|rail| {
                        layout::rail_tab_at(width, surface.dpi, progress, media, rail, (x, y))
                    });
                    let plain = !media && self.media.view().status().is_none();
                    surface.set_line_hover(
                        plain && layout::glance_at(width, surface.dpi, progress, (x, y)),
                    );
                    if tab != surface.tab_hover {
                        surface.set_tab_hover(tab);
                        surface.request(Repaint::Full);
                    }
                    self.pages.touch();
                    // Pointer activity restarts the doze deadline; eyes that dozed wave on the closed pill.
                    if let Some(eyes) = &mut self.pages.eyes
                        && eyes.touch(now, progress < 0.1)
                    {
                        surface.request(Repaint::Full);
                    }
                    if volume_ui {
                        let bar = layout::volume_at(width, surface.dpi, progress, x, y).is_some();
                        if surface.grip.drag {
                            if message.wParam.0 & MK_LBUTTON == 0 {
                                surface.end_grip(); // the release was lost
                            } else if let Some(volume) =
                                layout::volume_fraction(width, surface.dpi, progress, x)
                            {
                                self.audio.set(volume);
                            }
                        } else if bar != surface.grip.hover {
                            surface.grip.hover = bar;
                            surface.request(Repaint::Full);
                        }
                    }
                }
            }
            WM_LBUTTONDOWN if surface.tab_hover.is_some() => {
                RailFx::go(
                    &mut surface.rail_fx.press,
                    1.0,
                    motion::HOVER_IN_TIME / 3,
                    motion::SETTLE,
                );
                surface.request(Repaint::Full);
                surface.live = true;
            }
            WM_LBUTTONDOWN if volume_ui && matches!(surface.role, Role::Notch { .. }) => {
                let (x, y) = point(message.lParam);
                let (width, progress) =
                    (surface.bounds.width, surface.motion.value(Instant::now()));
                if layout::volume_at(width, surface.dpi, progress, x, y).is_some()
                    && let Some(volume) = layout::volume_fraction(width, surface.dpi, progress, x)
                {
                    // SAFETY: captures the pointer for this thread's own live notch window; released when
                    // the button goes up or the capture is lost.
                    unsafe {
                        SetCapture(surface.window.0);
                    }
                    surface.grip = Grip {
                        hover: true,
                        drag: true,
                    };
                    surface.request(Repaint::Full);
                    self.audio.set(volume);
                }
            }
            WM_CAPTURECHANGED => surface.end_grip(),
            WM_MOUSEWHEEL if volume_ui && surface.grip.hover => {
                let delta = i32::from((message.wParam.0 >> 16) as u16 as i16);
                let steps = edge::wheel_for(&mut self.wheel, edge::Wheel::Volume, delta);
                if steps != 0
                    && let Some(level) = self.audio.level()
                {
                    let fine = message.wParam.0 & 0x0004 != 0; // MK_SHIFT
                    self.audio
                        .set(f32::from(edge::step_level(level.percent, steps, fine)) / 100.0);
                }
            }
            WM_MOUSEWHEEL if surface.kind() == SurfaceKind::Dock => self.wheel_dock(message),
            // The open notch: the wheel flips the tabs, a sideways swipe on a live Music page skips the track.
            WM_MOUSEWHEEL if surface.expanded() => {
                let delta = i32::from((message.wParam.0 >> 16) as u16 as i16);
                let steps = edge::wheel_for(&mut self.wheel, edge::Wheel::Tabs, delta);
                if steps != 0 {
                    self.flip_tab(-steps);
                }
            }
            WM_MOUSEHWHEEL if surface.expanded() && volume_ui => {
                let delta = i32::from((message.wParam.0 >> 16) as u16 as i16);
                let now = Instant::now();
                if now >= self.swipe_until {
                    let (steps, rest) = edge::wheel_steps(self.swipe, delta);
                    self.swipe = rest;
                    if steps != 0 {
                        self.swipe = 0;
                        self.swipe_until = now + SWIPE_PAUSE;
                        self.media.command(if steps > 0 {
                            crate::media::NEXT
                        } else {
                            crate::media::PREVIOUS
                        });
                    }
                }
            }
            WM_MOUSELEAVE => {
                let tuck = match surface.kind() {
                    SurfaceKind::Dock => &mut self.tuck,
                    SurfaceKind::Notch => &mut self.tuck_notch,
                };
                tuck.release(&surface.monitor, Instant::now());
                surface.leave();
                if !surface.grip.drag && surface.grip.hover {
                    surface.grip.hover = false;
                    surface.request(Repaint::Full);
                }
            }
            WM_PAINT if self.graphics.removed() => self.lose_graphics(),
            _ => {}
        }
        Ok(())
    }
    /// Re-checks the full-screen monitor when the catalog changed or the shell poll is due, then
    /// hides that monitor's surfaces and restores the others, painted current before they appear.
    fn refresh_fullscreen(&mut self, generation: u64) -> Result<()> {
        let now = Instant::now();
        let polled = self.fullscreen_due.is_some_and(|due| now >= due);
        if self.fullscreen_generation == Some(generation) && !polled {
            return Ok(());
        }
        self.fullscreen_generation = Some(generation);
        let before = self.fullscreen.clone();
        // With "Hide in full screen" off nothing is ever remembered, so every surface stays shown.
        let hide = self.settings.hide_fullscreen;
        self.fullscreen.retain(|(key, device)| {
            hide && native_window::fullscreen_monitor(*key).as_ref() == Some(device)
        });
        let foreground = dock::foreground_key().filter(|_| hide);
        let candidate = foreground
            .and_then(|key| native_window::fullscreen_monitor(key).map(|device| (key, device)));
        let admitted = candidate
            .clone()
            .filter(|(key, _)| native_window::shell_fullscreen(*key));
        let waiting = candidate.is_some() && admitted.is_none();
        let normal_monitor = admitted
            .is_none()
            .then(|| foreground.and_then(native_window::foreground_monitor))
            .flatten();
        remember_fullscreen(
            &mut self.fullscreen,
            admitted,
            normal_monitor.as_deref(),
            &self.monitors,
        );
        self.fullscreen_due = (waiting && !polled).then_some(now + FULLSCREEN_POLL);
        keys::grip(); // the Caps grab follows full screen that starts without a focus change
        if before == self.fullscreen {
            return Ok(());
        }
        // A strip never shows over a full-screen app: the tuck pass drops it.
        self.tuck_generation = None;
        self.apply_visibility()
    }
    /// Hides or shows each surface for the full-screen and dock auto-hide state, painted current before
    /// it appears.
    fn apply_visibility(&mut self) -> Result<()> {
        let icons = self.dock_icons();
        let owners = self.interaction_owners();
        for surface in &mut self.surfaces {
            let held = owners.contains(&Some(surface.window.0));
            let visible = !self.hidden
                && !self
                    .fullscreen
                    .iter()
                    .any(|(_, device)| device == &surface.monitor)
                && match surface.kind() {
                    SurfaceKind::Dock => {
                        (self.tuck.shown(&surface.monitor) || held) && !self.dock_away
                    }
                    // An open notch stays up while a window slides under it.
                    SurfaceKind::Notch => {
                        self.tuck_notch.shown(&surface.monitor)
                            || surface.motion.target() > 0.0
                            || held
                    }
                };
            if visible && !surface.visible {
                match surface.kind() {
                    SurfaceKind::Notch => surface.paint_notch(
                        (&self.clock, &self.date),
                        &self.media,
                        &self.audio,
                        &self.timers,
                        &self.power,
                        &self.shelf,
                        &self.pages,
                    )?,
                    SurfaceKind::Dock => surface.paint_dock(
                        &self.dock_items,
                        &icons,
                        self.drag.as_ref(),
                        &self.pages.widget_readings,
                    )?,
                }
            }
            surface.overlay = false;
            surface.set_visible(visible);
        }
        Ok(())
    }
    fn fullscreen_wait(&self) -> u32 {
        self.fullscreen_due
            .map_or(INFINITE, |due| milliseconds_until(due, Instant::now()))
    }
    /// Milliseconds until the pending topology read, or `INFINITE` without one.
    fn topology_wait(&self) -> u32 {
        self.topology_due
            .map_or(INFINITE, |due| milliseconds_until(due, Instant::now()))
    }
    /// Applies a due topology read: releases surfaces of removed or changed monitors and places the
    /// shown kinds on added or changed ones. Unchanged monitors keep their surfaces and motion state.
    /// Transient reads and failed placements retry on a bounded schedule; existing surfaces stay.
    fn refresh_topology(&mut self) -> Result<()> {
        if self.topology_due.is_none_or(|due| due > Instant::now()) {
            return Ok(());
        }
        self.topology_due = None;
        let observed = native_window::monitors().and_then(|monitors| {
            topology::settle(monitors)
                .ok_or_else(|| Error::new(E_FAIL, "Display topology is changing."))
        });
        let next = match observed {
            Ok(next) => next,
            Err(error) => return self.retry_topology(error),
        };
        let (release, place) = topology::diff(&self.monitors, &next);
        if !release.is_empty() || !place.is_empty() {
            // A monitor change can move or end a full-screen app; re-check on the next pass.
            self.fullscreen_generation = None;
        }
        self.surfaces
            .retain(|surface| !release.contains(&surface.monitor));
        self.monitors = next;
        let mut failure = None;
        for monitor in place {
            for kind in self.shown.clone() {
                if let Err(error) = self.place(&monitor, kind) {
                    // Forget the monitor so a later read places all of its surfaces again.
                    self.surfaces.retain(|s| s.monitor != monitor.device);
                    self.monitors.retain(|m| m.device != monitor.device);
                    failure = Some(error);
                    break;
                }
            }
        }
        // After placement, and contained: a settings failure closes that window and never ends the host.
        let broken = self.settings_window.as_mut().is_some_and(|window| {
            window
                .refresh_monitors(&self.monitors)
                .map_err(|error| eprintln!("Settings monitors: {error}"))
                .is_err()
        });
        if broken {
            self.settings_window.take();
        }
        failure.map_or(Ok(()), |error| self.retry_topology(error))
    }
    /// Schedules a bounded re-read. Once retries run out with no surface left, `error` ends the host
    /// through the normal failure path instead of leaving it running invisibly.
    fn retry_topology(&mut self, error: Error) -> Result<()> {
        if self.topology_retries < TOPOLOGY_RETRIES {
            self.topology_retries += 1;
            self.topology_due = Some(Instant::now() + TOPOLOGY_RETRY);
        } else if self.surfaces.is_empty() {
            return Err(error);
        } else {
            // Remaining surfaces still forward the next display change, and Show commands re-read.
            eprintln!("Display topology: {error}");
        }
        Ok(())
    }
    /// The wake flags of the native windows and the keyboard watcher, read on every pass: Windows drops thread
    /// posts while a message box or a drag loop runs, and a flag left set would silence its feature for good.
    fn read_wakes(&mut self) {
        if native_window::clip_read() {
            crate::shot_os::check();
            self.capture_clip();
        }
        self.take_shots();
        if native_window::power_read() && self.power.update(power::Raw::read(), Instant::now()) {
            self.open_notches();
            self.request_notches(Repaint::Full, |_| true);
        }
        self.device_wakes();
        if native_window::wake_read() {
            let now = Instant::now();
            if let Some(eyes) = &mut self.pages.eyes {
                eyes.wake(now);
                self.request_notches(Repaint::Full, |s| !s.live && s.motion.value(now) < 0.1);
            }
        }
        if let Some((letter, answer)) = crate::system_os::ejected() {
            let text = answer.notice(letter);
            self.power
                .announce(text, Instant::now(), DEVICE_NOTICE, power::Extra::None);
            self.open_notches();
            self.request_notches(Repaint::Full, |_| true);
        }
        if keys::looking() {
            let said = self.keys.looked();
            self.key_pill(said);
        }
        self.worker_answers();
        if native_window::caps_read() {
            self.key_pill(Some(keys::caps_text()));
        }
        // A Windows notification's line and a finished file picker are read here, not on their wake: a modal loop can
        // swallow the wake, and the line would wait for the next toast.
        self.notification_peek();
        if let Some(paths) = self.file_picker.take() {
            if self.file_picker.for_sound {
                self.choose_sound(paths);
            } else {
                self.add_shelf_files(&paths);
            }
        }
    }
    /// Schedules a fresh topology read with a full retry budget.
    fn request_topology(&mut self) {
        self.topology_due = Some(Instant::now());
        self.topology_retries = 0;
    }
    /// One pass of state refreshes and frames; returns the clock, progress and visualizer wait and
    /// whether animation runs.
    fn update(&mut self, model: &WindowModel) -> Result<(u32, bool)> {
        self.refresh_topology()?;
        self.read_wakes();
        // The accent changed (the event only flagged it): rebuild like any palette change.
        let accent = self.text.take_colors_changed();
        if std::mem::take(&mut self.palette_dirty) || accent {
            let palette =
                crate::theme::system_palette(self.settings.theme, self.effective_accent());
            if palette != self.palette {
                self.palette = palette;
                self.menu = None;
                self.hover_popup = None;
                self.hover_spot = None;
                self.surfaces.clear();
                for kind in self.shown.clone() {
                    self.show(kind)?;
                }
            }
        }
        if self.text.take_changed() {
            self.rescale_text()?;
        }
        // A temporary tab choice ends once every notch has closed.
        if self.pages.tabs.choice.is_some() {
            let now = Instant::now();
            if self
                .surfaces
                .iter()
                .filter(|s| s.kind() == SurfaceKind::Notch)
                .all(|s| s.collapsed(now))
            {
                self.pages.tabs.choice = None;
            }
        }
        let media_on = self.pages.tabs.on(Tab::Media);
        self.media
            .set_expanded(media_on && self.surfaces.iter().any(Surface::expanded));
        // One paint per visible notch when the drawn track or status changes or its pause grace
        // expires, whatever else this pass asks (`paint_requested`). With the Media tab off the events are
        // still applied but draw nothing.
        let status = self.media.view().status();
        let changed =
            self.media.refresh(self.cover_pixels()) || status != self.media.view().status();
        let redraw = changed && media_on;
        if changed {
            self.peek_song();
        }
        let shown = self.pages.track(&self.media).is_some();
        if shown || self.settings.notch {
            // Core Audio loads once a notch can show the level: in a media panel, on the slider, or as a pill.
            self.audio.start();
        }
        let volume = self.audio.refresh();
        if volume {
            self.refresh_quick_menu();
        }
        if self.mic.refresh() && self.pages.dots.set_muted(self.mic.muted(), Instant::now()) {
            self.request_notches(Repaint::Full, |_| true);
        }
        if self.audio.elsewhere()
            && let Some(level) = self.audio.level()
        {
            let text = if level.muted {
                "Muted".to_owned()
            } else {
                format!("Volume {}%", level.percent)
            };
            self.power
                .announce(text, Instant::now(), VOLUME_NOTICE, power::Extra::None);
            self.open_notches();
            self.request_notches(Repaint::Full, |_| true);
        } else if redraw {
            self.request_notches(Repaint::Full, |_| true);
        } else if volume && shown {
            // Only an open or opening panel draws the volume row; a collapsed pill has nothing new.
            let now = Instant::now();
            self.request_notches(Repaint::Full, |s| motion::reveal(s.motion.value(now)) > 0.0);
        }
        self.refresh_dock(model)?;
        self.refresh_shelf();
        self.refresh_weather();
        self.refresh_limits();
        // The foreground window's own move or resize counts as a change too.
        let stamp = model.generation.wrapping_add(window_catalog::frames());
        self.refresh_fullscreen(stamp)?;
        let native_wait = self.refresh_interactions();
        let preview_wait = self.refresh_preview();
        let hover_wait = self.refresh_hover_popup();
        self.refresh_menu();
        let close_wait = self.refresh_close();
        self.refresh_tuck(stamp)?;
        let wait = self
            .refresh_clock()?
            .min(self.refresh_progress()?)
            .min(self.refresh_meter()?)
            .min(self.refresh_timers())
            .min(self.refresh_power())
            .min(preview_wait)
            .min(native_wait)
            .min(self.refresh_raise())
            .min(hover_wait)
            .min(self.refresh_levels())
            .min(self.refresh_eyes())
            .min(self.refresh_ring())
            .min(self.refresh_privacy())
            .min(self.refresh_prayer())
            .min(self.refresh_water())
            .min(self.refresh_reminders())
            .min(close_wait)
            .min(self.refresh_stats());
        self.refresh_widget_readings()?;
        self.paint_requested()?;
        if self
            .settings_window
            .as_ref()
            .is_some_and(SettingsWindow::closed)
        {
            self.settings_window.take();
        }
        // The Settings wheel glide takes the same frames as the surfaces, and none once it lands.
        let scrolling = self.settings_window.as_mut().is_some_and(|window| {
            window.glide_frame(Instant::now()).unwrap_or_else(|error| {
                eprintln!("Settings scroll: {error}");
                false
            })
        });
        let running = self.animate()? || scrolling;
        let wait = wait.min(self.refresh_footer_hover());
        Ok((wait, running))
    }
    /// A device loss schedules a rebuild and yields `fallback`; any other failure still ends the host.
    fn survive<T>(&mut self, result: Result<T>, fallback: T) -> Result<T> {
        match result {
            Err(error) if self.graphics.lost(&error) => {
                eprintln!("Graphics device lost: {error}");
                self.lose_graphics();
                Ok(fallback)
            }
            other => other,
        }
    }
    /// Releases every old-device surface now and schedules the rebuild.
    fn lose_graphics(&mut self) {
        self.surfaces.clear();
        self.menu = None;
        self.rebuild.lose(Instant::now());
    }
    /// Applies a due rebuild: a new device, then every shown kind placed and painted on it. Failures
    /// back off; once the budget is spent the error ends the host instead of leaving it invisible.
    fn refresh_graphics(&mut self) -> Result<()> {
        if !self.rebuild.start(Instant::now()) {
            return Ok(());
        }
        // Includes any surface a topology change placed on the lost device meanwhile.
        self.surfaces.clear();
        self.menu = None;
        // The old device is released by this assignment, after its surfaces and before any new one.
        let rebuilt = Graphics::create().and_then(|graphics| {
            self.graphics = graphics;
            self.shown
                .clone()
                .into_iter()
                .try_for_each(|kind| self.show(kind))
        });
        if let Err(error) = &rebuilt {
            eprintln!("Graphics rebuild: {error}");
            self.surfaces.clear();
        } else {
            // Windowless meanwhile, ROQA missed display, resume and time broadcasts, and a failed
            // topology placement may have dropped a monitor: re-read both.
            self.request_topology();
            self.clock_due = Instant::now();
            // The old window set may have missed media and audio events too, as at a resume.
            self.media.reconnect();
            self.audio.reconnect();
        }
        if self.rebuild.finish(rebuilt.is_ok(), Instant::now()) {
            Ok(())
        } else {
            rebuilt
        }
    }
    /// Re-reads the Windows text size after its change event and rebuilds the notches at the new scale.
    fn rescale_text(&mut self) -> Result<()> {
        let factor = self.text.factor()?;
        if factor == self.text_factor {
            return Ok(());
        }
        self.text_factor = factor;
        if let Some(window) = &mut self.settings_window {
            window.set_factor(factor)?;
        }
        self.surfaces.retain(|s| s.kind() != SurfaceKind::Notch);
        if self.shown.contains(&SurfaceKind::Notch) {
            self.show(SurfaceKind::Notch)?;
        }
        Ok(())
    }
}

pub fn run(probe: Probe, diagnostics: Option<std::path::PathBuf>) -> Result<()> {
    // Admission first: a second launch must not open (truncate) the running instance's log.
    let controls = Controls::create();
    if controls.as_ref().err().map(|e| e.code().0) != Some(crate::error_log::ALREADY_RUNNING) {
        crate::error_log::initialize();
    }
    let controls = controls?;
    let _apartment = Apartment::enter()?;
    let _class = WindowClass::register()?;
    let mut desktop = Desktop::create(&probe)?;
    #[cfg(debug_assertions)]
    if probe == Probe::DeviceLoss {
        let removed = windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED;
        desktop.survive(Err(Error::from(removed)), ())?;
    }
    #[cfg(debug_assertions)]
    if probe == Probe::Callback {
        // SAFETY: synchronous probe targets a live window on this thread; parameters contain no pointers.
        unsafe {
            SendMessageW(
                desktop.surfaces[0].window.0,
                native_window::PANIC_PROBE,
                Some(windows::Win32::Foundation::WPARAM(0)),
                Some(LPARAM(0)),
            );
        }
    }
    let report = diagnostics.or_else(|| {
        desktop
            .settings
            .diagnostics
            .as_ref()
            .map(std::path::PathBuf::from)
    });
    let mut catalog = Catalog::create(report)?;
    desktop.startup_notice();
    // One short worker reads the radios now, so the first Quick settings popup already has their rows.
    quick::refresh(None);
    crate::device_battery::start();
    crate::system_os::ensure_start_shortcut();
    crate::error_log::started();
    let result = event_loop(&controls, &mut desktop, &mut catalog);
    desktop.flush_store(true);
    result.and(catalog.shutdown())
}

fn event_loop(controls: &Controls, desktop: &mut Desktop, catalog: &mut Catalog) -> Result<()> {
    desktop.shell_hook = watch_flashes(desktop.wake.0);
    let mut events = controls.handles();
    let base = events.len();
    loop {
        // The registry watch's handle goes last, and only while the setting is on.
        events.truncate(base);
        events.extend(desktop.pages.watch.as_ref().map(privacy::Watch::handle));
        catalog.service()?;
        desktop.flush_store(false);
        desktop.refresh_graphics()?;
        let pass = desktop.update(catalog.model());
        let (clock_wait, animating) = desktop.survive(pass, (INFINITE, false))?;
        let timeout = catalog
            .wait_milliseconds()
            .min(clock_wait)
            .min(desktop.topology_wait())
            .min(desktop.fullscreen_wait())
            .min(desktop.tuck_wait())
            .min(desktop.store_wait())
            .min(desktop.media_wait())
            .min(desktop.rebuild.wait(Instant::now()));
        // Bits set after this pass's refresh whose wake was lost or not posted: run again now.
        let timeout = if desktop.media.pending() || desktop.audio.pending() {
            0
        } else {
            timeout
        };
        let wake = if animating {
            next_frame(&events, timeout)
        } else {
            // SAFETY: handles stay alive throughout the wait; the UI queue is pumped after every wake.
            unsafe {
                MsgWaitForMultipleObjectsEx(
                    Some(&events),
                    timeout,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                )
            }
        };
        if wake == WAIT_FAILED {
            return Err(Error::from_thread());
        }
        let index = wake.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
        if index == base {
            desktop.privacy_signal();
        } else if index < base {
            match Command::ALL[index] {
                Command::ShowSettings => {
                    if let Err(error) = desktop.open_settings() {
                        eprintln!("Settings: {error}");
                    }
                }
                Command::Agent => desktop.agent_reports(),
                command => {
                    let shown = desktop.command(command);
                    if !desktop.survive(shown, true)? {
                        return Ok(());
                    }
                }
            }
        }
        if !drain_messages(desktop, catalog)? || desktop.shown.is_empty() {
            return Ok(());
        }
    }
}

/// Asks the shell to post taskbar events (a window flashing, activating, closing) to the wake window; the loop reads
/// them before dispatch. Returns the `SHELLHOOK` message id, or 0 if refused.
fn watch_flashes(window: HWND) -> u32 {
    // SAFETY: the host's own live window, owned by this thread for the whole run.
    unsafe {
        let message = RegisterWindowMessageW(w!("SHELLHOOK"));
        if RegisterShellHookWindow(window).as_bool() {
            message
        } else {
            0
        }
    }
}

type CompositorClockWait = unsafe extern "system" fn(u32, *const HANDLE, u32) -> u32;

/// `DCompositionWaitForCompositorClock` exists only on Windows 11. It is resolved at run time so ROQA
/// still loads elsewhere and falls back to a short message wait there.
fn compositor_clock() -> Option<CompositorClockWait> {
    static CLOCK: OnceLock<Option<CompositorClockWait>> = OnceLock::new();
    *CLOCK.get_or_init(|| {
        // SAFETY: dcomp.dll is already loaded for DCompositionCreateDevice2; the export, when present,
        // has exactly this documented signature (count, handles, timeout) -> wait result.
        unsafe {
            let module = GetModuleHandleW(w!("dcomp.dll")).ok()?;
            let export = GetProcAddress(module, s!("DCompositionWaitForCompositorClock"))?;
            Some(std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                CompositorClockWait,
            >(export))
        }
    })
}

// https://learn.microsoft.com/windows/win32/directcomp/compositor-clock/compositor-clock
// The function reference also documents immediate occlusion status returns.
fn compositor_wake_supported(wake: u32, event_count: usize) -> bool {
    wake.wrapping_sub(WAIT_OBJECT_0.0) <= event_count as u32
        || wake == windows::Win32::Foundation::WAIT_TIMEOUT.0
}

/// While animating, sleeps until the next compositor frame or an IPC event; queued input is drained
/// right after, so it waits at most one frame. Idle returns to the plain blocking wait in `event_loop`.
fn next_frame(events: &[HANDLE], timeout: u32) -> WAIT_EVENT {
    // SAFETY: handles stay alive throughout either wait; neither wait dispatches callbacks.
    unsafe {
        if let Some(wait) = compositor_clock() {
            let wake = wait(events.len() as u32, events.as_ptr(), timeout.min(33));
            // Microsoft's compositor-clock example assigns the extra index to clock ticks.
            // Occlusion and any unrecognized NTSTATUS must take the bounded fallback below.
            if compositor_wake_supported(wake, events.len()) {
                return WAIT_EVENT(wake);
            }
        }
        MsgWaitForMultipleObjectsEx(
            Some(events),
            timeout.min(16),
            QS_ALLINPUT,
            MWMO_INPUTAVAILABLE,
        )
    }
}

fn drain_messages(desktop: &mut Desktop, catalog: &mut Catalog) -> Result<bool> {
    let mut message = MSG::default();
    let started = std::time::Instant::now();
    let mut dispatched = 0;
    // SAFETY: MSG output is valid; dispatch and all surface ownership stay on this UI thread.
    unsafe {
        while dispatched < 128
            && started.elapsed() < std::time::Duration::from_millis(4)
            && PeekMessageW(&mut message, None::<HWND>, 0, 0, PM_REMOVE).as_bool()
        {
            dispatched += 1;
            if message.message == WM_QUIT {
                if message.wParam.0 != 0 {
                    return Err(Error::new(
                        E_FAIL,
                        "Window callback failed; orderly shutdown completed.",
                    ));
                }
                return Ok(false);
            }
            if message.message == WM_HOTKEY {
                if keys::Keys::is_caps(message.wParam.0) {
                    desktop.key_pill(keys::hand_back().then(keys::caps_text));
                } else {
                    let shortcut = desktop.hotkey(message.wParam.0);
                    desktop.recover(shortcut, "Shortcut");
                }
                continue;
            }
            if message.message == toast::SHOWN || message.message == keys::LOOK {
                continue; // only wakes: `read_wakes` reads the line and the flag
            }
            if message.message == crate::tray::MENU {
                let picked = desktop.tray_menu();
                desktop.recover(picked, "Tray");
                continue;
            }
            if desktop.shell_hook != 0 && message.message == desktop.shell_hook {
                desktop.note_shell_event(message.wParam.0, message.lParam.0 as usize);
                continue;
            }
            if catalog.message(&message)? {
                continue;
            }
            if desktop
                .popup
                .as_mut()
                .is_some_and(|popup| popup.message(&message))
            {
                desktop.dismiss_preview();
                continue;
            }
            if desktop.hover_popup_message(&message, catalog) || desktop.menu_message(&message) {
                continue;
            }
            if matches!(
                message.message,
                WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
            ) && desktop.dock_at(message.hwnd).is_some()
            {
                desktop.dismiss_preview();
            }
            if message.message == WM_LBUTTONUP && desktop.dock_at(message.hwnd).is_some() {
                let hit = desktop
                    .dock_at(message.hwnd)
                    .and_then(|dock| dock.dock_hit(message.lParam, desktop.dock_slots()));
                let swallowed = desktop.swallow_release(message.hwnd, hit);
                if !desktop.end_drag(message.hwnd, message.lParam) && !swallowed {
                    let clicked = desktop.activate(message.hwnd, hit);
                    desktop.recover(clicked, "Dock click");
                }
                continue;
            }
            if message.message == WM_MBUTTONUP
                && let Some(dock) = desktop.dock_at(message.hwnd)
            {
                if let Some(slot) = dock
                    .dock_hit(message.lParam, desktop.dock_slots())
                    .and_then(Hit::slot)
                {
                    desktop.dismiss_preview();
                    desktop.middle_click_slot(slot);
                }
                continue;
            }
            if message.message == WM_RBUTTONUP
                && let Some(dock) = desktop.dock_at(message.hwnd)
            {
                let hit = dock.dock_hit(message.lParam, desktop.dock_slots());
                if hit == Some(Hit::Button(Button::Start)) {
                    desktop.open_windows_links(message.hwnd);
                    continue;
                }
                desktop.open_menu(message.hwnd, hit.and_then(Hit::slot), message.lParam);
                continue;
            }
            if message.message == WM_RBUTTONUP
                && desktop.shelf_menu_at(message.hwnd, message.lParam)
            {
                continue;
            }
            if message.message == WM_RBUTTONUP && desktop.is_notch(message.hwnd) {
                desktop.open_app_menu(message.hwnd, message.lParam);
                continue;
            }
            if desktop.tuck.message(&message, Instant::now())
                || desktop.tuck_notch.message(&message, Instant::now())
            {
                continue;
            }
            if desktop.notch_input(&message) {
                continue;
            }
            let settings = match &mut desktop.settings_window {
                Some(window) => window.message(&message)?,
                None => false,
            };
            if settings {
                let applied = desktop.settings_changed(catalog);
                desktop.survive(applied, ())?;
                continue;
            }
            if desktop.remind_message(&message)? {
                continue;
            }
            desktop.message(&message)?;
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(true)
}

/// Focus changes preserve other monitors; removed monitors and replacements cannot grow the set.
fn remember_fullscreen(
    current: &mut Vec<(WindowKey, String)>,
    candidate: Option<(WindowKey, String)>,
    normal_monitor: Option<&str>,
    monitors: &[Monitor],
) {
    // A normal app covering fullscreen on this monitor restores ROQA, as the native taskbar does.
    current.retain(|(_, device)| {
        normal_monitor != Some(device.as_str()) && monitors.iter().any(|m| &m.device == device)
    });
    if let Some((key, device)) = candidate
        && monitors.iter().any(|m| m.device == device)
    {
        current.retain(|(_, old)| old != &device);
        current.push((key, device));
    }
    sort::by(current, |a, b| a.1.cmp(&b.1));
}

#[cfg(test)]
mod clock_wait_tests {
    use super::*;
    use crate::window_model::Identity;

    #[test]
    fn ending_a_dock_drag_releases_its_capture_and_no_drag_leaves_others() {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetCapture;
        let _class = WindowClass::register().unwrap();
        let at = Bounds {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        let window = NativeWindow::create_hidden(at, w!("Drag test")).unwrap();
        // SAFETY: this thread's own hidden test window; it is never shown or given input.
        unsafe { SetCapture(window.0) };
        let mut none = None;
        assert!(take_drag(&mut none).is_none());
        // SAFETY: scalar capture query on this thread.
        assert_eq!(unsafe { GetCapture() }, window.0);
        let mut drag = Some(Drag {
            from: 0,
            x: 0,
            active: false,
            at: 0,
            gap: Tween::rest(0.0),
            group: None,
        });
        assert!(take_drag(&mut drag).is_some() && drag.is_none());
        // SAFETY: scalar capture query on this thread.
        assert!(unsafe { GetCapture() }.is_invalid());
    }

    #[test]
    fn icons_are_sized_for_the_dock_else_the_notch() {
        // A notch-only desktop (dock off) still gets shelf icons.
        assert_eq!(icon_dpi(Some(144), Some(96)), Some(144));
        assert_eq!(icon_dpi(None, Some(96)), Some(96));
        assert_eq!(icon_dpi(None, None), None);
    }

    #[test]
    fn click_mode_swells_a_closed_pill_and_never_shrinks_an_open_one() {
        assert!(swells(0.0));
        assert!(!swells(NOTCH_SWELL) && !swells(1.0));
    }

    #[test]
    fn a_pin_edit_marks_the_store_dirty_once_and_only_when_it_changed() {
        let (mut pins, mut store, mut pending) =
            (Pins::default(), Store::default(), Pending::default());
        let now = Instant::now();
        let app = |id: &str| Identity::Application(id.into());
        for id in ["a", "b", "c"] {
            assert!(edit_pins(&mut pins, &mut store, &mut pending, now, |p| p
                .add(app(id))
                .is_ok()));
        }
        assert_eq!(store.pins, ["app:a", "app:b", "app:c"]);
        // Nothing unwritten means no deadline; no-op edits change neither the store nor the deadline.
        let mut pending = Pending::default();
        assert_eq!(pending.decision(now), Flush::Idle);
        let before = store.clone();
        for (from, to) in [(1, 1), (0, 3), (7, 0)] {
            assert!(!edit_pins(&mut pins, &mut store, &mut pending, now, |p| p
                .move_to(from, to)));
        }
        assert!(!edit_pins(&mut pins, &mut store, &mut pending, now, |p| p
            .add(app("a"))
            .is_ok()));
        assert!(!edit_pins(&mut pins, &mut store, &mut pending, now, |p| p
            .remove(&app("missing"))));
        assert_eq!(
            (store.clone(), pending.decision(now)),
            (before, Flush::Idle)
        );
        // One real reorder: one rewrite, one pending change due a coalesce delay later.
        assert!(edit_pins(&mut pins, &mut store, &mut pending, now, |p| p
            .move_to(0, 2)));
        assert_eq!(store.pins, ["app:b", "app:c", "app:a"]);
        assert_eq!(pending.decision(now), Flush::Wait(now + store::COALESCE));
    }
    #[test]
    fn a_shelf_edit_marks_the_store_dirty_once_and_only_when_it_changed() {
        let mut view = ShelfView {
            removed: None,
            shelf: Shelf::default(),
            clips: Clips::default(),
            copied: false,
            shots: crate::shots::Shots::default(),
        };
        let (mut store, mut pending, now) = (Store::default(), Pending::default(), Instant::now());
        let mut add = |view: &mut ShelfView, list: &[&str]| {
            edit_shelf(view, &mut store, &mut pending, now, |s| s.add_all(list) > 0)
        };
        // Nothing valid in a drop: no change, no dirty mark, no icon request.
        assert!(!add(&mut view, &["relative.txt", r"\\.\PhysicalDrive0"]));
        assert!(add(&mut view, &[r"C:\a.txt", r"C:\b.txt"]));
        // The same files again are duplicates: nothing to write.
        assert!(!add(&mut view, &[r"C:\A.TXT"]));
        assert_eq!(store.shelf, [r"C:\a.txt", r"C:\b.txt"]);
        assert_eq!(pending.decision(now), Flush::Wait(now + store::COALESCE));
        // A removal is one more change; marking a target missing is not persisted, so not an edit here.
        let mut pending = Pending::default();
        assert!(edit_shelf(&mut view, &mut store, &mut pending, now, |s| s.remove(0)));
        assert_eq!(store.shelf, [r"C:\b.txt"]);
        assert_eq!(pending.decision(now), Flush::Wait(now + store::COALESCE));
    }

    #[test]
    fn the_meter_runs_only_open_playing_and_metered_and_never_above_15_hz() {
        let now = Instant::now();
        // Collapsed, hidden and full-screen-suppressed notches are not open (`Surface::expanded`).
        for (open, playing, metered) in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
            (false, false, false),
        ] {
            assert_eq!(meter_due(open, playing, metered, None, now), None);
            assert_eq!(meter_due(open, playing, metered, Some(now), now), None);
        }
        assert_eq!(meter_due(true, true, true, None, now), Some(now));
        // Wakes early, on time, late and after a stop and restart: samples stay a period apart.
        let mut sampled = None;
        let mut samples = Vec::new();
        for (wake, open) in [
            (0, true),
            (10, true),
            (67, true),
            (300, true),
            (310, false),
            (320, true),
            (366, true),
            (367, true),
            (1000, true),
            // Resumed an hour later: one sample, not a catch-up burst, then the period again.
            (3_601_000, true),
            (3_601_001, true),
            (3_601_066, true),
            (3_601_067, true),
        ] {
            let at = now + Duration::from_millis(wake);
            if meter_due(open, true, true, sampled, at).is_some_and(|due| due <= at) {
                sampled = Some(at);
                samples.push(wake);
            }
        }
        assert_eq!(samples, [0, 67, 300, 367, 1000, 3_601_000, 3_601_067]);
        assert!(samples.windows(2).all(|pair| pair[1] - pair[0] >= 67));
    }

    #[test]
    fn a_pass_paints_each_notch_once_by_its_strongest_request() {
        // A tick, a meter sample and a minute in one pass: the full paint covers the bars.
        let mut asked = Repaint::default();
        assert_eq!(asked, Repaint::Nothing);
        for request in [Repaint::Meter, Repaint::Full, Repaint::Meter] {
            asked = asked.max(request);
        }
        assert_eq!(asked, Repaint::Full);
        // A bars-only pass repaints only the meter slot, and taking the request resets it.
        let mut asked = Repaint::Nothing.max(Repaint::Meter);
        assert_eq!(std::mem::take(&mut asked), Repaint::Meter);
        assert_eq!(asked, Repaint::Nothing);
    }

    #[test]
    fn fullscreen_focus_changes_preserve_monitors_and_state_is_bounded() {
        let monitors: Vec<_> = ["A", "B"]
            .into_iter()
            .map(|device| Monitor {
                device: device.into(),
                work: Bounds {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
                rect: Bounds {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
                dpi: 96,
                taskbar: 0,
            })
            .collect();
        let candidate = |handle, device: &str| {
            (
                WindowKey {
                    handle,
                    process: 10,
                },
                device.to_owned(),
            )
        };
        let mut current = vec![];
        remember_fullscreen(&mut current, Some(candidate(1, "A")), None, &monitors);
        remember_fullscreen(&mut current, None, None, &monitors);
        assert_eq!(current, [candidate(1, "A")]);
        remember_fullscreen(&mut current, Some(candidate(2, "B")), None, &monitors);
        assert_eq!(current, [candidate(1, "A"), candidate(2, "B")]);
        remember_fullscreen(&mut current, Some(candidate(3, "A")), None, &monitors);
        assert_eq!(current, [candidate(3, "A"), candidate(2, "B")]);
        remember_fullscreen(&mut current, Some(candidate(4, "C")), None, &monitors[1..]);
        assert_eq!(current, [candidate(2, "B")]);
        // ROQA foreground has no foreign monitor and retains the entry.
        remember_fullscreen(&mut current, None, None, &monitors);
        assert_eq!(current, [candidate(2, "B")]);
        // Normal foreground on the other monitor retains fullscreen B.
        remember_fullscreen(&mut current, None, Some("A"), &monitors);
        assert_eq!(current, [candidate(2, "B")]);
        // Alt-Tab to a normal app on B releases that monitor's suppression.
        remember_fullscreen(&mut current, None, Some("B"), &monitors);
        assert!(current.is_empty());
        remember_fullscreen(&mut current, Some(candidate(2, "B")), None, &monitors);
        remember_fullscreen(&mut current, None, None, &[]);
        assert!(current.is_empty());
    }

    #[test]
    fn graphics_rebuilds_back_off_to_a_cap() {
        let delays: Vec<u128> = (0..8).map(|n| rebuild_delay(n).as_millis()).collect();
        assert_eq!(delays, [250, 500, 1000, 2000, 4000, 8000, 8000, 8000]);
        assert_eq!(rebuild_delay(u32::MAX).as_millis(), 8000);
    }
    #[test]
    fn rebuilds_keep_one_pending_spend_a_budget_and_refresh_once_settled() {
        let start = Instant::now();
        let mut rebuild = Rebuild::new(start);
        assert!(!rebuild.start(start));
        rebuild.lose(start);
        rebuild.lose(start + Duration::from_millis(200));
        assert_eq!(rebuild.due, Some(start + GRAPHICS_RETRY));
        assert!(!rebuild.start(start + Duration::from_millis(249)));
        // Every failure backs off further until the budget is spent.
        let mut now = start + GRAPHICS_RETRY;
        for attempt in 1..GRAPHICS_ATTEMPTS {
            assert!(rebuild.start(now));
            assert!(rebuild.finish(false, now));
            assert_eq!(rebuild.due, Some(now + rebuild_delay(attempt)));
            now += rebuild_delay(attempt);
        }
        assert!(rebuild.start(now));
        assert!(!rebuild.finish(false, now));
        // Success clears the schedule; a loss soon after keeps backing off, a settled one starts over.
        let mut rebuild = Rebuild::new(start);
        rebuild.lose(start);
        assert!(rebuild.start(start + GRAPHICS_RETRY));
        assert!(rebuild.finish(true, start + GRAPHICS_RETRY));
        assert_eq!(rebuild.wait(start), INFINITE);
        let soon = start + Duration::from_secs(1);
        rebuild.lose(soon);
        assert_eq!(rebuild.due, Some(soon + rebuild_delay(1)));
        let mut rebuild = Rebuild::new(start);
        rebuild.lose(start);
        assert!(rebuild.start(start + GRAPHICS_RETRY));
        assert!(rebuild.finish(true, start + GRAPHICS_RETRY));
        let later = start + GRAPHICS_RETRY + GRAPHICS_SETTLED;
        rebuild.lose(later);
        assert_eq!(rebuild.due, Some(later + GRAPHICS_RETRY));
    }
    #[test]
    fn accepts_events_clock_and_timeout_but_falls_back_on_occlusion_or_failure() {
        for index in 0..=4 {
            assert!(compositor_wake_supported(WAIT_OBJECT_0.0 + index, 4));
        }
        assert!(compositor_wake_supported(
            windows::Win32::Foundation::WAIT_TIMEOUT.0,
            4
        ));
        for status in [WAIT_FAILED.0, 0xC01E0006, 0x80, 5] {
            assert!(!compositor_wake_supported(status, 4));
        }
    }
    #[test]
    fn the_handle_follows_the_shown_tabs_and_the_active_page() {
        let mut settings = Settings::default();
        // Enabled tabs show a handle, on Media while a session lives, else on the last tab used.
        let pages = Pages::new(&settings, Tab::Shelf);
        let rail = |pages: &Pages, live| pages.rail(live).map(|r| (r.count, r.active));
        assert_eq!(rail(&pages, true), Some((5, 0)));
        assert_eq!(rail(&pages, false), Some((5, 4)));
        assert_eq!(pages.page(false), Tab::Shelf);
        // A pick during one open wins until it ends.
        settings.tabs[Tab::Timer.index()] = false;
        settings.tabs[Tab::Weather.index()] = false;
        let mut pages = Pages::new(&settings, Tab::Media);
        assert_eq!(rail(&pages, false), Some((3, 0)));
        assert!(pages.tabs.select(Tab::Shelf, true));
        assert_eq!(rail(&pages, true), Some((3, 2)));
        // One tab: no handle. None: the plain clock panel is the Media page.
        settings.tabs = [true, false, false, false, false, false, false];
        assert_eq!(rail(&Pages::new(&settings, Tab::Media), true), None);
        settings.tabs = [false; 7];
        let none = Pages::new(&settings, Tab::Shelf);
        assert_eq!((rail(&none, true), none.page(true)), (None, Tab::Media));
    }

    #[test]
    fn the_mascot_has_no_state_with_the_setting_off() {
        let mut settings = Settings::default();
        assert!(Pages::new(&settings, Tab::Media).eyes.is_some());
        settings.eyes = false;
        assert!(Pages::new(&settings, Tab::Media).eyes.is_none());
    }
}
