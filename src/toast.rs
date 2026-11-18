//! Windows notifications as a short peek on the notch row, event-only and installing nothing: the shell shows each
//! banner in one `Windows.UI.Core.CoreWindow` of ShellExperienceHost that stays alive and is only cloaked while no
//! banner shows. One out-of-context WinEvent for that window being uncloaked says "a banner is up"; a short-lived
//! thread then reads the banner's app name, title and first line through UI Automation (the shell's own labels) and
//! hands one line to the host. The text lives only in memory until the host takes it; it is never stored or logged.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering::Relaxed},
    },
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            Threading::GetCurrentThreadId,
        },
        UI::{
            Accessibility::{
                CUIAutomation, HWINEVENTHOOK, IUIAutomation, SetWinEventHook, TreeScope_Subtree,
                UnhookWinEvent,
            },
            WindowsAndMessaging::{
                EVENT_OBJECT_UNCLOAKED, GetClassNameW, GetWindowThreadProcessId,
                PostThreadMessageW, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_APP,
            },
        },
    },
    core::Result,
};

/// Posted to the host thread when a banner's line is ready: `take` it.
pub const SHOWN: u32 = WM_APP + 21;
static HOST: AtomicU32 = AtomicU32::new(0);
/// One banner is read at a time; a second shown meanwhile is the same stack and is dropped.
static READING: AtomicBool = AtomicBool::new(false);
static LINE: Mutex<Option<String>> = Mutex::new(None);
/// The longest line kept, in characters; the row trims the rest with an ellipsis.
const LONGEST: usize = 140;

/// The latest banner's line, once.
pub fn take() -> Option<String> {
    LINE.lock().ok()?.take()
}

unsafe extern "system" fn uncloaked(
    _: HWINEVENTHOOK,
    _: u32,
    window: HWND,
    object: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    if object != 0 || child != 0 {
        return;
    }
    let mut class = [0u16; 32];
    let mut process = 0;
    // SAFETY: a bounded class-name read and a scalar process-id query of the window the event names.
    let length = unsafe {
        GetWindowThreadProcessId(window, Some(&mut process));
        GetClassNameW(window, &mut class)
    };
    // The same class holds Start, Search and the quick settings; only the shell's toast host qualifies.
    if String::from_utf16_lossy(&class[..length.max(0) as usize]) != "Windows.UI.Core.CoreWindow"
        || !crate::window_source::process_executable(process)
            .is_some_and(|path| path.ends_with("\\shellexperiencehost.exe"))
        || READING.swap(true, Relaxed)
    {
        return;
    }
    let handle = window.0 as usize;
    if std::thread::Builder::new()
        .spawn(move || read(handle))
        .is_err()
    {
        READING.store(false, Relaxed);
    }
}

/// One banner read on its own thread (UI Automation can wait on the shell, so never on the host thread).
fn read(handle: usize) {
    // SAFETY: COM is initialized and released on this thread, after every object of `banner` is gone.
    let line = unsafe {
        let started = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        // shortcut: the banner's text fills in a moment after it is shown; a few short retries, only on this event.
        let mut line = None;
        for _ in 0..8 {
            line = banner(HWND(handle as _)).ok().flatten();
            if line.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(60));
        }
        if started {
            CoUninitialize();
        }
        line
    };
    READING.store(false, Relaxed);
    let host = HOST.load(Relaxed);
    if let (Some(line), true) = (line, host != 0) {
        if let Ok(mut slot) = LINE.lock() {
            *slot = Some(line);
        }
        // SAFETY: a scalar-only message to the host thread.
        let _ = unsafe { PostThreadMessageW(host, SHOWN, WPARAM(0), LPARAM(0)) };
    }
}

/// "App: Title, first line" from the shell's labelled text blocks of the banner; none when the window is not one.
fn banner(window: HWND) -> Result<Option<String>> {
    // SAFETY: UI Automation reads of another process's window; every returned object is released on drop.
    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let all = automation
            .ElementFromHandle(window)?
            .FindAll(TreeScope_Subtree, &automation.CreateTrueCondition()?)?;
        let (mut app, mut title, mut text) = (String::new(), String::new(), String::new());
        for index in 0..all.Length()? {
            let element = all.GetElement(index)?;
            let slot = match element.CurrentAutomationId()?.to_string().as_str() {
                "SenderName" => &mut app,
                "Title" => &mut title,
                "MessageText" => &mut text,
                _ => continue,
            };
            *slot = element.CurrentName()?.to_string();
        }
        Ok(line(&app, &title, &text))
    }
}

/// The parts that exist joined as "App: Title, text", one line, nothing for a window that had no title or text.
fn line(app: &str, title: &str, text: &str) -> Option<String> {
    let one = |part: &str| part.split_whitespace().collect::<Vec<_>>().join(" ");
    let (app, title, text) = (one(app), one(title), one(text));
    if title.is_empty() && text.is_empty() {
        return None;
    }
    let body = [title, text]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let line = if app.is_empty() {
        body
    } else {
        format!("{app}: {body}")
    };
    Some(line.chars().take(LONGEST).collect())
}

/// The hook for banners being shown; dropping it removes the hook, so a switched-off peek costs nothing.
pub struct Toasts(HWINEVENTHOOK);

impl Toasts {
    /// A refused hook only loses the peek.
    pub fn new() -> Self {
        // SAFETY: a scalar thread id; the hook is out-of-context (no DLL is injected) and delivered on this
        // thread's message loop; a refused hook comes back null.
        let hook = unsafe {
            HOST.store(GetCurrentThreadId(), Relaxed);
            SetWinEventHook(
                EVENT_OBJECT_UNCLOAKED,
                EVENT_OBJECT_UNCLOAKED,
                None,
                Some(uncloaked),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        Self(hook)
    }
}

impl Drop for Toasts {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: the hook this struct registered.
            let _ = unsafe { UnhookWinEvent(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_banner_reads_as_one_line_with_what_it_has() {
        assert_eq!(
            line("Mail", "Alice", "Lunch at noon?").as_deref(),
            Some("Mail: Alice, Lunch at noon?")
        );
        assert_eq!(line("", "Alice", "").as_deref(), Some("Alice"));
        assert_eq!(
            line("Mail", "A\nB", " x\r\ny ").as_deref(),
            Some("Mail: A B, x y")
        );
        assert_eq!(
            line("Start", "", " "),
            None,
            "no title and no text is no banner"
        );
        assert_eq!(
            line("", &"a".repeat(500), "").unwrap().chars().count(),
            LONGEST
        );
    }
}
