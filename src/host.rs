use crate::{
    clock,
    dock::{self, DockItem},
    dock_uia::{self, DockAccessibility},
    graphics::{Canvas, DockScene, DockSlot, Graphics, NotchScene},
    icons::{self, IconService},
    ipc::{Command, Controls},
    layout::{self, Bounds, SurfaceKind, placement},
    motion::{self, Tween},
    native_window::{self, NativeWindow, WindowClass},
    picker::Picker,
    text_settings::TextSettings,
    window_catalog::Catalog,
    window_model::WindowModel,
};
use std::{
    mem::size_of,
    sync::OnceLock,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{E_FAIL, HANDLE, HWND, LPARAM, WAIT_EVENT, WAIT_FAILED, WAIT_OBJECT_0, WPARAM},
    System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
    System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
        WindowsAndMessaging::*,
    },
};
use windows::core::{Error, Result, s, w};

#[derive(PartialEq, Eq)]
pub enum Probe {
    None,
    Initialization,
    Callback,
}

struct Apartment;
impl Apartment {
    fn enter() -> Result<Self> {
        // SAFETY: main thread owns all COM objects and drops them before Apartment.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: balances successful initialization on this same thread.
        unsafe {
            CoUninitialize();
        }
    }
}

struct Surface {
    // Declared first: UIA must release the dock before its HWND is destroyed.
    _accessible: Option<DockAccessibility>,
    canvas: Canvas,
    window: NativeWindow,
    kind: SurfaceKind,
    hovered: bool,
}
impl Surface {
    fn create(
        graphics: &Graphics,
        kind: SurfaceKind,
        dpi: u32,
        work: Bounds,
        slots: usize,
        uia: &dock_uia::Shared,
    ) -> Result<Self> {
        let bounds = placement(work, dpi, kind, slots);
        let window = NativeWindow::create(bounds, kind)?;
        let accessible = match kind {
            SurfaceKind::Dock => Some(DockAccessibility::create(window.0, uia)?),
            SurfaceKind::Notch => {
                // Collapsed at rest: only the visible notch takes input.
                window.set_region(Some(layout::notch_region(bounds, dpi, 0.0)))?;
                None
            }
        };
        let canvas = Canvas::create(graphics, window.0, bounds)?;
        window.show();
        Ok(Self {
            _accessible: accessible,
            canvas,
            window,
            kind,
            hovered: false,
        })
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
}

/// Hover-intent swell: the pill grows ~11 x 3 DIP to acknowledge the pointer before opening.
const NOTCH_SWELL: f32 = 0.07;

struct Desktop {
    surfaces: Vec<Surface>,
    graphics: Graphics,
    dpi: u32,
    work: Bounds,
    picker: Option<Picker>,
    icons: IconService,
    dock_items: Vec<DockItem>,
    dock_generation: Option<u64>,
    dock_hover: Option<usize>,
    /// Per-slot hover progress, and the pointer-inside progress of the bar fill.
    dock_motion: Vec<Tween>,
    dock_inside: Tween,
    /// 0 collapsed to 1 expanded.
    notch_motion: Tween,
    /// Pointer inside the notch's hot band (not its shadow).
    notch_engaged: bool,
    /// Hover intent: the notch opens at this instant if the pointer is still engaged.
    notch_intent: Option<Instant>,
    /// Frames are painted only while these are set; `animate` clears them when motion settles.
    notch_live: bool,
    dock_live: bool,
    uia: dock_uia::Shared,
    /// Window region last applied to the notch; it follows the drawn shape frame by frame.
    notch_area: Option<Bounds>,
    /// Windows text size (Accessibility > Text size) scales the whole notch, whose only content is text.
    text: TextSettings,
    text_factor: f64,
    /// Last drawn time text; the notch repaints only when it changes.
    clock: String,
    date: String,
}
impl Desktop {
    fn create(probe: &Probe) -> Result<Self> {
        let work = native_window::primary_work_area()?;
        let text = TextSettings::create()?;
        let text_factor = text.factor()?;
        let mut desktop = Self {
            surfaces: Vec::new(),
            graphics: Graphics::create()?,
            dpi: native_window::primary_dpi(work)?,
            work,
            picker: None,
            icons: IconService::create()?,
            dock_items: Vec::new(),
            dock_generation: None,
            dock_hover: None,
            dock_motion: Vec::new(),
            dock_inside: Tween::rest(0.0),
            notch_motion: Tween::rest(0.0),
            notch_engaged: false,
            notch_intent: None,
            notch_live: false,
            dock_live: false,
            uia: Arc::new(Mutex::new(dock_uia::State::default())),
            notch_area: None,
            text,
            text_factor,
            clock: String::new(),
            date: String::new(),
        };
        desktop.show(SurfaceKind::Notch)?;
        if *probe == Probe::Initialization {
            return Err(Error::new(
                E_FAIL,
                "Injected failure after first composed window.",
            ));
        }
        desktop.show(SurfaceKind::Dock)?;
        Ok(desktop)
    }
    fn dock_slots(&self) -> usize {
        self.dock_items.len() + 1
    }
    fn show(&mut self, kind: SurfaceKind) -> Result<()> {
        if !self.surfaces.iter().any(|surface| surface.kind == kind) {
            let surface = Surface::create(
                &self.graphics,
                kind,
                match kind {
                    SurfaceKind::Notch => self.notch_dpi(),
                    SurfaceKind::Dock => self.dpi,
                },
                self.work,
                self.dock_slots(),
                &self.uia,
            )?;
            self.surfaces.push(surface);
            if kind == SurfaceKind::Dock {
                self.reset_dock_motion();
                self.dock_inside = Tween::rest(0.0);
                self.publish_accessibility(None, false);
                self.paint_dock()?;
            } else {
                self.notch_motion = Tween::rest(0.0);
                (self.notch_engaged, self.notch_intent) = (false, None);
                self.notch_area = Some(layout::notch_region(
                    self.notch_window(),
                    self.notch_dpi(),
                    0.0,
                ));
                self.notch_live = false;
                let now = clock::now();
                (self.clock, self.date) = (now.time, now.date);
                self.paint_notch()?;
            }
        }
        Ok(())
    }
    /// The notch's effective DPI: monitor DPI times the Windows text-size factor.
    fn notch_dpi(&self) -> u32 {
        (f64::from(self.dpi) * self.text_factor).round() as u32
    }
    fn notch_window(&self) -> Bounds {
        placement(self.work, self.notch_dpi(), SurfaceKind::Notch, 0)
    }
    fn notch(&self) -> Option<&Surface> {
        self.surfaces
            .iter()
            .find(|surface| surface.kind == SurfaceKind::Notch)
    }
    /// Repaints the notch only when the minute changed; returns how long until the next minute.
    fn refresh_clock(&mut self) -> Result<u32> {
        let now = clock::now();
        if now.time != self.clock {
            (self.clock, self.date) = (now.time, now.date);
            self.paint_notch()?;
        }
        Ok(now.next_minute_ms)
    }
    fn paint_notch(&self) -> Result<()> {
        match self.notch() {
            Some(notch) => notch.canvas.paint_notch(&NotchScene {
                time: &self.clock,
                date: &self.date,
                progress: self.notch_motion.value(Instant::now()),
                dpi: self.notch_dpi(),
            }),
            None => Ok(()),
        }
    }
    /// Pointer entered or left the notch. Entering swells the pill at once and opens it only after
    /// `INTENT_TIME` (a pointer passing along the screen edge never pops the panel open); leaving
    /// closes from wherever the motion is. The HWND itself never moves.
    fn set_notch_hover(&mut self, engaged: bool) -> Result<()> {
        if engaged == self.notch_engaged {
            return Ok(());
        }
        self.notch_engaged = engaged;
        if !engaged {
            self.notch_intent = None;
            self.notch_motion
                .retarget(0.0, motion::CLOSE_TIME, motion::CLOSE);
        } else if self.notch_motion.value(Instant::now()) > NOTCH_SWELL {
            // Re-entered while still closing: reopen directly.
            self.open_notch();
        } else {
            self.notch_intent = Some(Instant::now() + motion::INTENT_TIME);
            self.notch_motion
                .retarget(NOTCH_SWELL, motion::INTENT_TIME, motion::EASE_OUT);
        }
        self.notch_live = true;
        Ok(())
    }
    fn open_notch(&mut self) {
        self.notch_intent = None;
        self.notch_motion
            .retarget(1.0, motion::OPEN_TIME, motion::OPEN);
    }
    /// Paints one frame of each running animation; returns true while any still runs.
    /// The final frame always lands exactly on the target before a surface goes idle.
    fn animate(&mut self) -> Result<bool> {
        let now = Instant::now();
        if self.notch_intent.is_some_and(|at| now >= at) {
            self.open_notch();
        }
        if self.notch_live {
            self.paint_notch()?;
            self.fit_notch_region(now)?;
            self.notch_live = self.notch_motion.running(now) || self.notch_intent.is_some();
        }
        if self.dock_live {
            self.paint_dock()?;
            self.dock_live = self.dock_inside.running(now)
                || self.dock_motion.iter().any(|tween| tween.running(now));
        }
        Ok(self.notch_live || self.dock_live)
    }
    /// Keeps the window region around what is drawn. While growing it leads by two frames, because
    /// region and composition updates are not synchronized; a larger region only adds transparent pixels.
    fn fit_notch_region(&mut self, now: Instant) -> Result<()> {
        let progress = self
            .notch_motion
            .value(now)
            .max(self.notch_motion.value(now + Duration::from_millis(33)));
        let area = layout::notch_region(self.notch_window(), self.notch_dpi(), progress);
        if self.notch_area != Some(area) {
            if let Some(notch) = self.notch() {
                notch.window.set_region(Some(area))?;
            }
            self.notch_area = Some(area);
        }
        Ok(())
    }
    fn reset_dock_motion(&mut self) {
        self.dock_hover = None;
        self.dock_motion = vec![Tween::rest(0.0); self.dock_slots()];
    }
    /// Highlights the hovered slot (or none) and the bar fill while the pointer is inside.
    fn hover_dock(&mut self, slot: Option<usize>, inside: bool) {
        let ease = |tween: &mut Tween, on: bool| {
            let (target, duration) = if on {
                (1.0, motion::HOVER_IN_TIME)
            } else {
                (0.0, motion::HOVER_OUT_TIME)
            };
            tween.retarget(target, duration, motion::EASE_OUT);
        };
        ease(&mut self.dock_inside, inside);
        for (index, tween) in self.dock_motion.iter_mut().enumerate() {
            ease(tween, slot == Some(index));
        }
        self.dock_hover = slot;
        self.dock_live = true;
    }
    fn close(&mut self, kind: SurfaceKind) {
        self.surfaces.retain(|surface| surface.kind != kind);
    }
    fn dock(&self) -> Option<&Surface> {
        self.surfaces
            .iter()
            .find(|surface| surface.kind == SurfaceKind::Dock)
    }
    fn dock_bounds(&self) -> Bounds {
        placement(self.work, self.dpi, SurfaceKind::Dock, self.dock_slots())
    }
    fn open_picker(&mut self, catalog: &Catalog) -> Result<()> {
        self.picker.take();
        self.picker = Some(Picker::create(catalog.model())?);
        Ok(())
    }
    /// Rebuilds dock items only when the catalog generation or an icon changed.
    fn refresh_dock(&mut self, model: &WindowModel) -> Result<()> {
        let icons_arrived = self.icons.collect();
        if self.dock_generation == Some(model.generation) && !icons_arrived {
            return Ok(());
        }
        if self.dock_generation != Some(model.generation) {
            self.dock_generation = Some(model.generation);
            let items = dock::items(model);
            let changed = items != self.dock_items;
            self.publish_accessibility(Some(dock::names(&items, model)), changed);
            // Title-only changes publish new generations; they rename UIA items but leave pixels untouched.
            if !changed && !icons_arrived {
                return Ok(());
            }
            let resized = items.len() != self.dock_items.len();
            self.dock_items = items;
            if resized {
                self.reset_dock_motion();
                let bounds = self.dock_bounds();
                if let Some(surface) = self
                    .surfaces
                    .iter_mut()
                    .find(|s| s.kind == SurfaceKind::Dock)
                {
                    surface.window.resize(bounds)?;
                    surface.canvas.resize(bounds)?;
                }
            }
        }
        self.paint_dock()
    }
    /// Shares slot names/geometry with UIA providers; a layout change retires every older element.
    fn publish_accessibility(&self, names: Option<Vec<String>>, layout_changed: bool) {
        if let Ok(mut state) = self.uia.lock() {
            state.dpi = self.dpi;
            if let Some(names) = names {
                state.names = names;
            } else if state.names.is_empty() {
                state.names = dock::names(&self.dock_items, &WindowModel::default());
            }
            if layout_changed {
                state.version = state.version.wrapping_add(1);
            }
        }
    }
    fn paint_dock(&mut self) -> Result<()> {
        let size = dock::px(dock::ICON_DIPS, self.dpi) as u32;
        let pixels: Vec<_> = self
            .dock_items
            .iter()
            .map(|item| {
                let sources = icons::sources(&item.identity, item.executable.as_deref());
                self.icons.icon(&item.identity, sources, size)
            })
            .collect();
        let Some(surface) = self.dock() else {
            return Ok(());
        };
        let now = Instant::now();
        let hover = |slot: usize| self.dock_motion.get(slot).map_or(0.0, |t| t.value(now));
        let mut slots = vec![DockSlot {
            icon: None,
            active: false,
            picker: true,
            hover: hover(0),
        }];
        slots.extend(self.dock_items.iter().zip(&pixels).enumerate().map(
            |(index, (item, icon))| DockSlot {
                icon: icon.as_deref(),
                active: item.active,
                picker: false,
                hover: hover(index + 1),
            },
        ));
        surface.canvas.paint_dock(&DockScene {
            slots,
            inside: self.dock_inside.value(now),
            dpi: self.dpi,
        })
    }
    fn dock_slot(&self, lparam: LPARAM) -> Option<usize> {
        let x = i32::from((lparam.0 & 0xffff) as u16 as i16);
        dock::slot_at(x, self.dock_bounds().width, self.dpi, self.dock_slots())
    }
    /// A UIA Invoke from an element of the current layout acts exactly like a click on that slot.
    fn invoke_dock(&mut self, slot: usize, version: u32, catalog: &Catalog) -> Result<()> {
        let current = self.uia.lock().map(|state| state.version).ok();
        if current == Some(version) {
            self.activate_slot(Some(slot), catalog)?;
        }
        Ok(())
    }
    /// Slot 0 opens the accessible picker; other slots act on their real window group.
    fn activate_slot(&mut self, slot: Option<usize>, catalog: &Catalog) -> Result<()> {
        match slot {
            Some(0) => self.open_picker(catalog),
            Some(slot) => {
                if let Some(action) = self
                    .dock_items
                    .get(slot - 1)
                    .and_then(|item| dock::action(item, dock::foreground_key()))
                {
                    // A refused or stale target is left alone; the catalog repaints the dock on the next event.
                    dock::perform(action);
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
    fn command(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::Quit => return Ok(false),
            Command::ShowPicker => {}
            Command::ShowNotch => self.show(SurfaceKind::Notch)?,
            Command::ShowDock => self.show(SurfaceKind::Dock)?,
            Command::HideNotch => self.close(SurfaceKind::Notch),
            Command::HideDock => self.close(SurfaceKind::Dock),
        }
        Ok(!self.surfaces.is_empty())
    }
    fn message(&mut self, message: &MSG) -> Result<()> {
        if message.message == native_window::REPOSITION {
            return self.reposition(message.wParam);
        }
        let Some(index) = self
            .surfaces
            .iter()
            .position(|surface| surface.window.0 == message.hwnd)
        else {
            return Ok(());
        };
        let kind = self.surfaces[index].kind;
        match message.message {
            native_window::CLOSE_SURFACE => self.close(kind),
            WM_MOUSEMOVE => {
                let entered = self.surfaces[index].track_pointer()?;
                if kind == SurfaceKind::Dock {
                    let slot = self.dock_slot(message.lParam);
                    if entered || slot != self.dock_hover {
                        self.hover_dock(slot, true);
                    }
                } else {
                    let x = i32::from((message.lParam.0 & 0xffff) as u16 as i16);
                    let y = i32::from((message.lParam.0 >> 16) as u16 as i16);
                    let progress = self.notch_motion.value(Instant::now());
                    let width = self.notch_window().width;
                    let over = layout::notch_hit(width, self.notch_dpi(), progress, x, y);
                    self.set_notch_hover(over)?;
                }
            }
            WM_MOUSELEAVE => {
                if self.surfaces[index].hovered {
                    self.surfaces[index].hovered = false;
                    if kind == SurfaceKind::Dock {
                        self.hover_dock(None, false);
                    } else {
                        self.set_notch_hover(false)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn reposition(&mut self, dpi: WPARAM) -> Result<()> {
        let work = native_window::primary_work_area()?;
        let dpi = if dpi.0 == 0 {
            native_window::primary_dpi(work)?
        } else {
            (dpi.0 & 0xffff) as u32
        };
        if self.dpi == dpi && self.work == work {
            return Ok(());
        }
        self.dpi = dpi;
        self.work = work;
        self.rebuild()
    }
    /// Re-reads the Windows text size after its change event and rebuilds the notch at the new scale.
    fn rescale_text(&mut self) -> Result<()> {
        let factor = self.text.factor()?;
        if factor == self.text_factor {
            return Ok(());
        }
        self.text_factor = factor;
        self.rebuild()
    }
    fn rebuild(&mut self) -> Result<()> {
        self.dock_hover = None;
        let kinds: Vec<_> = self.surfaces.iter().map(|surface| surface.kind).collect();
        self.surfaces.clear();
        for kind in kinds {
            self.show(kind)?;
        }
        Ok(())
    }
}

pub fn run(probe: Probe, diagnostics: Option<std::path::PathBuf>) -> Result<()> {
    let _apartment = Apartment::enter()?;
    let _class = WindowClass::register()?;
    let controls = Controls::create()?;
    let mut desktop = Desktop::create(&probe)?;
    if probe == Probe::Callback {
        // SAFETY: synchronous probe targets a live window on this thread; parameters contain no pointers.
        unsafe {
            SendMessageW(
                desktop.surfaces[0].window.0,
                native_window::PANIC_PROBE,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            );
        }
    }
    let mut catalog = Catalog::create(diagnostics)?;
    let result = event_loop(&controls, &mut desktop, &mut catalog);
    result.and(catalog.shutdown())
}

fn event_loop(controls: &Controls, desktop: &mut Desktop, catalog: &mut Catalog) -> Result<()> {
    let events = controls.handles();
    loop {
        catalog.service()?;
        if desktop.text.take_changed() {
            desktop.rescale_text()?;
        }
        desktop.refresh_dock(catalog.model())?;
        let clock_wait = desktop.refresh_clock()?;
        if let Some(picker) = &mut desktop.picker {
            picker.synchronize(catalog.model())?;
            if picker.closed() {
                desktop.picker.take();
            }
        }
        let timeout = catalog.wait_milliseconds().min(clock_wait);
        let wake = if desktop.animate()? {
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
        if index < events.len() {
            if matches!(Command::ALL[index], Command::ShowPicker) {
                desktop.open_picker(catalog)?;
            } else if !desktop.command(Command::ALL[index])? {
                return Ok(());
            }
        }
        if !drain_messages(desktop, catalog)? || desktop.surfaces.is_empty() {
            return Ok(());
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

/// While animating, sleeps until the next compositor frame or an IPC event; queued input is drained
/// right after, so it waits at most one frame. Idle returns to the plain blocking wait in `event_loop`.
fn next_frame(events: &[HANDLE], timeout: u32) -> WAIT_EVENT {
    // SAFETY: handles stay alive throughout either wait; neither wait dispatches callbacks.
    unsafe {
        if let Some(wait) = compositor_clock() {
            let wake = wait(events.len() as u32, events.as_ptr(), timeout.min(33));
            if wake != WAIT_FAILED.0 {
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
            if catalog.message(&message)? {
                continue;
            }
            if message.message == WM_LBUTTONUP
                && desktop
                    .dock()
                    .is_some_and(|dock| dock.window.0 == message.hwnd)
            {
                let slot = desktop.dock_slot(message.lParam);
                desktop.activate_slot(slot, catalog)?;
                continue;
            }
            if message.message == dock_uia::INVOKE
                && desktop
                    .dock()
                    .is_some_and(|dock| dock.window.0 == message.hwnd)
            {
                desktop.invoke_dock(message.wParam.0, message.lParam.0 as u32, catalog)?;
                continue;
            }
            if let Some(picker) = &mut desktop.picker {
                if picker.message(&message)? {
                    continue;
                }
                if picker.keyboard(&message) {
                    continue;
                }
            }
            desktop.message(&message)?;
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(true)
}
