//! Native shell popups share the host's WinEvent stream; only requested, identified windows hold a surface.
use crate::{dock, layout::Bounds, window_model::WindowKey, window_source};
use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};
const OPEN_WAIT: Duration = Duration::from_secs(2);

use windows::Win32::{
    Foundation::{HWND, RECT},
    System::SystemInformation::GetTickCount,
    UI::{
        Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_B, VK_ESCAPE, VK_LWIN, VK_RETURN, VK_S},
        WindowsAndMessaging::*,
    },
};

/// An anchored interaction cannot be opened without its originating surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owner(pub HWND);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Start,
    Tray,
    Search,
}
impl Kind {
    fn keys(self) -> &'static [VIRTUAL_KEY] {
        match self {
            Self::Start => &[VK_LWIN],
            Self::Tray => &[VK_LWIN, VK_B],
            Self::Search => &[VK_LWIN, VK_S],
        }
    }
    fn class(self) -> &'static str {
        match self {
            Self::Tray => "TopLevelWindowForOverflowXamlIsland",
            Self::Start | Self::Search => "Windows.UI.Core.CoreWindow",
        }
    }
    fn matches(self, class: &str, executable: &str) -> bool {
        let name = executable.rsplit('\\').next().unwrap_or(executable);
        class == self.class()
            && name
                == match self {
                    Self::Tray => "explorer.exe",
                    Self::Start => "startmenuexperiencehost.exe",
                    Self::Search => "searchhost.exe",
                }
    }
}
#[derive(Clone, Copy)]
struct Event {
    kind: u32,
    window: HWND,
    time: u32,
}
thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static OVERFLOW: Cell<bool> = const { Cell::new(false) };
}
/// Called by the existing catalog hook on the UI thread. No window queries inside the callback.
pub fn event(kind: u32, window: HWND, time: u32) {
    if ACTIVE.get() {
        EVENTS.with_borrow_mut(|events| {
            if events.len() < 256 {
                events.push(Event { kind, window, time });
            } else {
                OVERFLOW.set(true);
            }
        });
    }
}
struct Request {
    owner: Owner,
    kind: Kind,
    since: u32,
    due: Option<Instant>,
    window: Option<WindowKey>,
    tray_entered: bool,
    anchor: (i32, i32),
    work: Bounds,
}
#[derive(Debug, PartialEq, Eq)]
enum Transition {
    None,
    Focus,
    Visible,
    Moved,
    Closed,
}
impl Request {
    fn current(&self, event: Event) -> bool {
        let elapsed = event.time.wrapping_sub(self.since);
        elapsed <= i32::MAX as u32
            && (self.window.is_some() || elapsed <= OPEN_WAIT.as_millis() as u32)
    }
    fn observe(&mut self, event: Event, candidate: Option<WindowKey>) -> Transition {
        if !self.current(event) {
            return Transition::None;
        }
        if event.kind == EVENT_SYSTEM_FOREGROUND {
            return Transition::Focus;
        }
        if let Some(key) = self.window {
            if event.window.0 as usize == key.handle && event.kind == EVENT_OBJECT_LOCATIONCHANGE {
                return Transition::Moved;
            }
            return if event.window.0 as usize == key.handle
                && matches!(
                    event.kind,
                    EVENT_OBJECT_HIDE
                        | EVENT_OBJECT_DESTROY
                        | EVENT_OBJECT_CLOAKED
                        | EVENT_SYSTEM_MENUPOPUPEND
                ) {
                Transition::Closed
            } else {
                Transition::None
            };
        }
        if event.kind == EVENT_OBJECT_SHOW
            && let Some(key) = candidate
        {
            self.window = Some(key);
            self.due = None;
            return Transition::Visible;
        }
        Transition::None
    }
}
#[derive(Default)]
pub struct Native {
    request: Option<Request>,
    location: Option<crate::window_catalog::Hook>,
}
impl Native {
    pub fn open(&mut self, owner: Owner, kind: Kind, anchor: (i32, i32), work: Bounds) -> bool {
        // SAFETY: scalar monotonic tick query, same time base as WinEvent's timestamp.
        let since = unsafe { GetTickCount() };
        if !self.begin(Request {
            owner,
            kind,
            since,
            due: Some(Instant::now() + OPEN_WAIT),
            window: None,
            tray_entered: false,
            anchor,
            work,
        }) {
            return false;
        }
        if !dock::send_keys(kind.keys()) {
            self.cancel();
            return false;
        }
        // Win+B may have completed synchronously; otherwise its foreground event advances the request.
        self.tray_focus();
        true
    }
    fn begin(&mut self, request: Request) -> bool {
        // Keep the first interaction; a concurrent click must not steal its owner or events.
        if self.request.is_some() {
            return false;
        }
        EVENTS.with_borrow_mut(Vec::clear);
        OVERFLOW.set(false);
        self.request = Some(request);
        ACTIVE.set(true);
        true
    }
    pub fn owner(&self) -> Option<Owner> {
        self.request.as_ref().map(|r| r.owner)
    }
    /// The panel `owner` opened as `kind`, once it shows (not while it is still opening).
    fn shown(&self, owner: Owner, kind: Kind) -> Option<WindowKey> {
        self.request
            .as_ref()
            .filter(|r| r.owner == owner && r.kind == kind)
            .and_then(|r| r.window)
    }
    /// A second click on the button whose panel shows: true when it does, and Esc closes it if it holds the
    /// foreground, the panel's own close. Its hide event then releases the request like any dismissal.
    pub fn close_shown(&self, owner: Owner, kind: Kind) -> bool {
        let Some(window) = self.shown(owner, kind) else {
            return false;
        };
        // SAFETY: scalar foreground query; the key goes only to the panel that is in front.
        if unsafe { GetForegroundWindow() }.0 as usize == window.handle
            && !dock::send_keys(&[VK_ESCAPE])
        {
            eprintln!("Shell popup: Windows refused the Esc key");
        }
        true
    }
    pub fn cancel(&mut self) {
        self.request = None;
        self.location = None;
        ACTIVE.set(false);
        EVENTS.with_borrow_mut(Vec::clear);
    }
    pub fn refresh(&mut self, now: Instant) -> Option<Instant> {
        let events = EVENTS.with_borrow_mut(std::mem::take);
        if OVERFLOW.replace(false) {
            eprintln!("Shell popup: event queue overflow; releasing owner");
            self.cancel();
        }
        for event in events {
            self.observe(event);
        }
        if self
            .request
            .as_ref()
            .is_some_and(|r| r.due.is_some_and(|due| now >= due))
        {
            eprintln!("Shell popup: did not appear; releasing owner");
            self.cancel();
        }
        self.request.as_ref().and_then(|r| r.due)
    }
    fn tray_focus(&mut self) {
        let Some(r) = self
            .request
            .as_mut()
            .filter(|r| r.kind == Kind::Tray && !r.tray_entered)
        else {
            return;
        };
        // SAFETY: scalar foreground query; Enter is sent only while the real taskbar owns focus.
        let foreground = unsafe { GetForegroundWindow() };
        if matches!(
            class_of(foreground).as_str(),
            "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) && explorer_window(foreground)
        {
            r.tray_entered = true;
            if !dock::send_keys(&[VK_RETURN]) {
                self.cancel();
            }
        }
    }
    // Retain tray positioning only after identifying the exact requested Explorer window.
    fn observe(&mut self, event: Event) {
        let Some(r) = self.request.as_mut() else {
            return;
        };
        let candidate = (event.kind == EVENT_OBJECT_SHOW && r.window.is_none() && r.current(event))
            .then(|| identify(event.window, r.kind))
            .flatten();
        match r.observe(event, candidate) {
            Transition::Closed => self.cancel(),
            Transition::Focus => self.tray_focus(),
            Transition::Visible if r.kind == Kind::Tray => {
                if let Some(key) = r.window {
                    match crate::window_catalog::popup_locations(key) {
                        Ok(hook) => self.location = Some(hook),
                        Err(error) => eprintln!("Tray placement hook: {error}"),
                    }
                }
                if let Some(key) = r.window {
                    place_tray(key, r.anchor, r.work);
                }
            }
            Transition::Moved if r.kind == Kind::Tray => {
                if let Some(key) = r.window {
                    place_tray(key, r.anchor, r.work);
                }
            }
            _ => {}
        }
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn class_of(window: HWND) -> String {
    let mut class = [0u16; 64];
    // SAFETY: writes only into the supplied local buffer.
    let len = unsafe { GetClassNameW(window, &mut class) }.max(0) as usize;
    String::from_utf16_lossy(&class[..len])
}
fn identify(window: HWND, kind: Kind) -> Option<WindowKey> {
    if class_of(window) != kind.class() {
        return None;
    }
    let mut process = 0;
    // SAFETY: scalar queries of a candidate from a show event, validated before use.
    unsafe {
        if !IsWindowVisible(window).as_bool() {
            return None;
        }
        GetWindowThreadProcessId(window, Some(&mut process));
    }
    if kind == Kind::Tray && !explorer_window(window) {
        return None;
    }
    let executable = window_source::process_executable(process)?;
    kind.matches(&class_of(window), &executable)
        .then_some(WindowKey {
            handle: window.0 as usize,
            process,
        })
}
fn explorer_window(window: HWND) -> bool {
    let (mut shell, mut candidate) = (0, 0);
    // SAFETY: scalar process-id queries; a missing shell cannot be a source.
    unsafe {
        GetWindowThreadProcessId(GetShellWindow(), Some(&mut shell));
        GetWindowThreadProcessId(window, Some(&mut candidate));
    }
    shell != 0 && shell == candidate
}
fn place_tray(key: WindowKey, anchor: (i32, i32), work: Bounds) {
    let window = HWND(key.handle as *mut _);
    let mut rect = RECT::default();
    let mut process = 0;
    // SAFETY: the exact source identity is revalidated; the foreign move is asynchronous and never activates it.
    unsafe {
        GetWindowThreadProcessId(window, Some(&mut process));
        if process != key.process || class_of(window) != Kind::Tray.class() {
            return;
        }
        if GetWindowRect(window, &mut rect).is_err() {
            return;
        }
        let (x, y) = tray_at(rect, anchor, work);
        if (rect.left, rect.top) == (x, y) {
            return;
        }
        let _ = SetWindowPos(
            window,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
        );
    }
}

fn tray_at(rect: RECT, anchor: (i32, i32), work: Bounds) -> (i32, i32) {
    let width = rect.right - rect.left;
    (
        (anchor.0 - width / 2).clamp(work.x, (work.x + work.width - width).max(work.x)),
        (anchor.1 - (rect.bottom - rect.top)).max(work.y),
    )
}
#[cfg(test)]
mod tests;
