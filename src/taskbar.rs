//! Settings "Hide Windows taskbar": while ROQA runs, every taskbar (the main one and one per extra monitor) is hidden
//! and set to auto-hide, so its reserved space goes back to apps. Explorer show events re-hide the exact taskbar.
//! ROQA restores the owner's auto-hide choice when it quits, signs out or turns the switch off. If ROQA crashes
//! or is killed, the watcher (`--taskbar <pid> <state>`) restores it after the process exits.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use windows::Win32::{
    Foundation::{CloseHandle, HWND, LPARAM},
    System::Threading::{
        CreateProcessW, GetCurrentProcess, INFINITE, OpenProcess, PROCESS_CREATION_FLAGS,
        PROCESS_INFORMATION, PROCESS_SYNCHRONIZE, STARTUPINFOW, SetProcessWorkingSetSize,
        TerminateProcess, WaitForSingleObject,
    },
    UI::{
        Shell::{ABM_GETSTATE, ABM_SETSTATE, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage},
        WindowsAndMessaging::{
            FindWindowExW, GetClassNameW, GetShellWindow, GetWindowThreadProcessId,
            IsWindowVisible, SW_HIDE, SW_SHOWNOACTIVATE, ShowWindowAsync,
        },
    },
};
use windows::core::{PCWSTR, PWSTR, w};

/// ROQA has the taskbar hidden now; the owner's appbar state to put back is `SAVED`.
static HIDDEN: AtomicBool = AtomicBool::new(false);
static SAVED: AtomicU32 = AtomicU32::new(0);
const TRAY_CLASSES: [PCWSTR; 2] = [w!("Shell_TrayWnd"), w!("Shell_SecondaryTrayWnd")];

fn appbar(message: u32, state: u32) -> u32 {
    let mut data = APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        lParam: LPARAM(state as isize),
        ..Default::default()
    };
    // SAFETY: `data` is a live, sized APPBARDATA; the shell reads or fills it in place.
    unsafe { SHAppBarMessage(message, &mut data) as u32 }
}

/// Shows or hides every taskbar window.
fn show_all(show: bool) {
    for class in TRAY_CLASSES {
        let mut after = None;
        // SAFETY: a top-level window search and show by class; a window that vanished meanwhile only fails the call.
        while let Ok(window) = unsafe { FindWindowExW(None, after, class, None) } {
            // SAFETY: as above; showing or hiding another process's window is a plain posted request.
            let _ =
                unsafe { ShowWindowAsync(window, if show { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
            after = Some(window);
        }
    }
}

pub(crate) fn explorer_window(window: HWND) -> bool {
    let (mut shell, mut candidate) = (0, 0);
    // SAFETY: scalar process-id queries; a missing shell cannot be a source.
    unsafe {
        GetWindowThreadProcessId(GetShellWindow(), Some(&mut shell));
        GetWindowThreadProcessId(window, Some(&mut candidate));
    }
    shell != 0 && shell == candidate
}

fn is_taskbar_class(class: &[u16]) -> bool {
    // SAFETY: TRAY_CLASSES contains only static NUL-terminated strings.
    TRAY_CLASSES
        .iter()
        .any(|name| class == unsafe { name.as_wide() })
}

pub(crate) fn is_taskbar(window: HWND) -> bool {
    let mut class = [0u16; 32];
    // SAFETY: writes only into the supplied buffer; an invalid window returns zero.
    let length = unsafe { GetClassNameW(window, &mut class) }.max(0) as usize;
    is_taskbar_class(&class[..length]) && explorer_window(window)
}

pub(crate) fn shown(window: HWND) {
    if !hidden() || !is_taskbar(window) {
        return;
    }
    // SAFETY: visibility query and asynchronous hide work on foreign windows without waiting on Explorer.
    unsafe {
        if IsWindowVisible(window).as_bool() {
            let _ = ShowWindowAsync(window, SW_HIDE);
        }
    }
}

/// The state with auto-hide on, which the shell then gives back to apps as work area.
fn hiding(state: u32) -> u32 {
    state | ABS_AUTOHIDE
}

/// True while ROQA suppresses the Windows taskbar, so nothing has to reserve space for it.
pub fn hidden() -> bool {
    HIDDEN.load(Relaxed)
}

/// Hides the taskbar again after Explorer made a new one (a restart, a new monitor); nothing when not hidden.
pub fn hide_again() {
    if hidden() {
        show_all(false);
    }
}

/// Puts the owner's taskbar back as it was; safe to call twice, and from the sign-out message.
pub fn restore() {
    if HIDDEN.swap(false, Relaxed) {
        put_back(SAVED.load(Relaxed));
    }
}

fn put_back(state: u32) {
    appbar(ABM_SETSTATE, state);
    show_all(true);
}

/// Starts the watcher: this exe with `--taskbar <pid> <state>`, inheriting no handle.
fn start_watcher(state: u32) -> Option<crate::ipc::OwnedHandle> {
    let exe = std::env::current_exe().ok()?;
    let line = format!(
        "\"{}\" --taskbar {} {state}",
        exe.display(),
        std::process::id()
    );
    let mut line: Vec<u16> = line.encode_utf16().chain([0]).collect();
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: a writable NUL-terminated command line and live startup and process storage; the thread handle is
    // closed here and the process handle is owned by the result.
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(line.as_mut_ptr())),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(0),
            None,
            PCWSTR::null(),
            &startup,
            &mut process,
        )
        .ok()?;
        let _ = CloseHandle(process.hThread);
    }
    Some(crate::ipc::OwnedHandle(process.hProcess))
}

/// The taskbar hidden for as long as this lives, with its watcher.
pub struct Taskbar {
    watcher: Option<crate::ipc::OwnedHandle>,
}

impl Taskbar {
    pub fn hide() -> Self {
        let state = appbar(ABM_GETSTATE, 0);
        // A watcher that will not start costs only the crash-safety; quit and sign-out still restore.
        let watcher = start_watcher(state);
        SAVED.store(state, Relaxed);
        HIDDEN.store(true, Relaxed);
        if hiding(state) != state {
            appbar(ABM_SETSTATE, hiding(state));
        }
        show_all(false);
        Self { watcher }
    }
}

impl Drop for Taskbar {
    fn drop(&mut self) {
        restore();
        if let Some(watcher) = &self.watcher {
            // SAFETY: the watcher's own process handle, closed by its owner after this.
            let _ = unsafe { TerminateProcess(watcher.0, 0) };
        }
    }
}

/// The watcher: waits for ROQA (`pid`) to end however it ends, then puts the taskbar back to `state`.
pub fn watch(pid: &str, state: &str) -> windows::core::Result<()> {
    let (Ok(pid), Ok(state)) = (pid.parse::<u32>(), state.parse::<u32>()) else {
        return Err(windows::core::Error::new(
            windows::Win32::Foundation::E_INVALIDARG,
            "Use: --taskbar <pid> <state>.",
        ));
    };
    // SAFETY: a synchronize-only handle, waited on and closed here. ROQA already gone fails the open: restore now.
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            // Asleep for hours: hand back the pages start-up touched.
            let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
            #[expect(
                clippy::disallowed_methods,
                reason = "the watcher process has no UI thread; waiting for ROQA to end is its only job"
            )]
            let _ = WaitForSingleObject(process, INFINITE);
            let _ = CloseHandle(process);
        }
    }
    put_back(state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_primary_and_secondary_taskbar_classes_are_suppressed() {
        for (name, expected) in [
            ("Shell_TrayWnd", true),
            ("Shell_SecondaryTrayWnd", true),
            ("NotifyIconOverflowWindow", false),
            ("Shell_TrayWndExtra", false),
            ("", false),
        ] {
            assert_eq!(
                is_taskbar_class(&name.encode_utf16().collect::<Vec<_>>()),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn hiding_adds_auto_hide_and_keeps_the_rest() {
        const ALWAYS_ON_TOP: u32 = 2;
        assert_eq!(hiding(0), ABS_AUTOHIDE);
        assert_eq!(hiding(ALWAYS_ON_TOP), ALWAYS_ON_TOP | ABS_AUTOHIDE);
        assert_eq!(
            hiding(ABS_AUTOHIDE),
            ABS_AUTOHIDE,
            "already auto-hide: no change"
        );
    }

    #[test]
    fn a_bad_watcher_line_is_refused_before_anything_runs() {
        assert!(watch("x", "0").is_err() && watch("1", "").is_err());
    }
}
