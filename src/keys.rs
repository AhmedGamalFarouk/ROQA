//! The keyboard language and Caps Lock pills, event-only: ROQA never sees an ordinary key press (no keyboard hook,
//! no raw input). Language: one out-of-context WinEvent for a new foreground window (the input switcher's flyout
//! is one, and so is the window it hands the focus back to) says "look again"; the answer is the foreground
//! thread's keyboard layout. Caps Lock: one `RegisterHotKey` for that key alone; when it fires the key is handed
//! back with `SendInput`, so it still toggles, and the pill says the new state.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND},
    Globalization::GetLocaleInfoW,
    Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
    System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, OpenProcess, OpenProcessToken,
        PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
        Input::KeyboardAndMouse::{
            GetKeyState, GetKeyboardLayout, HOT_KEY_MODIFIERS, INPUT, INPUT_0, INPUT_KEYBOARD,
            KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, MOD_NOREPEAT, RegisterHotKey,
            SendInput, UnregisterHotKey, VK_CAPITAL,
        },
        WindowsAndMessaging::{
            EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetWindowThreadProcessId,
            WINEVENT_OUTOFCONTEXT, WM_APP,
        },
    },
};

/// Posted to the host thread when the foreground window changed: read the layout and the Caps Lock grip again.
pub const LOOK: u32 = WM_APP + 20;
static HOST: AtomicU32 = AtomicU32::new(0);
/// A wake is posted once until the host reads it.
static LOOKING: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn foreground(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    let host = HOST.load(Relaxed);
    if host != 0 && !LOOKING.swap(true, Relaxed) && !crate::native_window::post_wake(host, LOOK) {
        LOOKING.store(false, Relaxed);
    }
}

/// Whether a foreground change waits to be read; the host asks on every pass, so a dropped post costs nothing.
pub fn looking() -> bool {
    LOOKING.load(Relaxed)
}

/// Whether the Caps Lock hotkey is registered now: off while an elevated window or a full-screen app has the focus,
/// where it could not be handed back. A static, since the window procedure hands the key back in a modal loop too.
static HELD: AtomicBool = AtomicBool::new(false);
/// The Settings switch "Caps Lock pill": off, the key is never taken.
static WANTED: AtomicBool = AtomicBool::new(true);

/// Turns the Caps Lock pill on or off (`Settings::apply_globals`); the grip follows at once.
pub fn want_caps(on: bool) {
    if WANTED.swap(on, Relaxed) != on {
        grip();
    }
}

/// The hotkey id of Caps Lock, clear of the shortcut groups' ids (`hotkeys::id` stays below 0x100).
const CAPS: i32 = 0x1000;
/// Caps Lock's scan code, so the handed-back key reads like the real one.
const CAPS_SCAN: u16 = 0x3A;
/// A language's own name ("English", "العربية") from the low word of a keyboard layout.
fn language_name(layout: isize) -> String {
    const LOCALE_SNATIVELANGUAGENAME: u32 = 0x4;
    let mut name = [0u16; 80];
    // SAFETY: a locale query into a bounded local buffer; the length includes the terminator.
    let length = unsafe {
        GetLocaleInfoW(
            (layout & 0xFFFF) as u32,
            LOCALE_SNATIVELANGUAGENAME,
            Some(&mut name),
        )
    };
    String::from_utf16_lossy(&name[..(length.max(1) as usize - 1).min(name.len())])
}

/// Whether the process behind `process` (a handle that may query it) runs elevated.
fn token_elevated(process: HANDLE) -> bool {
    let mut token = HANDLE::default();
    let mut info = TOKEN_ELEVATION::default();
    let mut size = 0;
    // SAFETY: a token query into locals; the token handle opened here is closed before returning.
    unsafe {
        if OpenProcessToken(process, TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let read = GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut info as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        let _ = CloseHandle(token);
        read.is_ok() && info.TokenIsElevated != 0
    }
}

/// Whether the window's process runs elevated while ROQA does not: Windows drops input injected into it, so
/// Caps Lock could not be handed back there. A process that cannot be queried counts as not elevated.
fn elevated_above_us(window: HWND) -> bool {
    let mut process = 0;
    // SAFETY: scalar queries; the process handle opened here is closed before returning.
    unsafe {
        GetWindowThreadProcessId(window, Some(&mut process));
        if process == 0 || process == GetCurrentProcessId() {
            return false;
        }
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) else {
            return false;
        };
        let elevated = token_elevated(handle);
        let _ = CloseHandle(handle);
        elevated && !token_elevated(GetCurrentProcess())
    }
}
fn caps_on() -> bool {
    // SAFETY: a scalar key-state read.
    unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) & 1 != 0 }
}

fn register_caps() -> bool {
    // SAFETY: a hotkey of the wake window with scalar arguments.
    unsafe {
        RegisterHotKey(
            crate::native_window::wake_window(),
            CAPS,
            HOT_KEY_MODIFIERS(MOD_NOREPEAT.0),
            u32::from(VK_CAPITAL.0),
        )
        .is_ok()
    }
}

/// Holds Caps Lock unless an elevated window or a full-screen app (a game, a call) has the focus, and releases it
/// there. The host asks at a focus change and on its full-screen pass.
pub fn grip() {
    let full_screen = crate::native_window::fullscreen_candidate()
        .is_some_and(|(key, _)| crate::native_window::shell_fullscreen(key));
    // SAFETY: a scalar foreground query.
    let want = WANTED.load(Relaxed)
        && !(full_screen || elevated_above_us(unsafe { GetForegroundWindow() }));
    if want == HELD.load(Relaxed) {
        return;
    }
    if !want {
        // SAFETY: a hotkey of the wake window with scalar arguments.
        let _ = unsafe { UnregisterHotKey(crate::native_window::wake_window(), CAPS) };
    }
    HELD.store(want && register_caps(), Relaxed);
}

/// Caps Lock was pressed and swallowed by the hotkey: lets go of it, hands the key back at once so it toggles, and
/// takes it again. Every press is handed back as it comes, never counted for later. False when Windows refused the
/// injected key (nothing toggled, so the hotkey stays released until the focus moves).
pub fn hand_back() -> bool {
    // SAFETY: scalar hotkey calls; the two inputs are plain keyboard events.
    let sent = unsafe {
        let _ = UnregisterHotKey(crate::native_window::wake_window(), CAPS);
        let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_CAPITAL,
                    wScan: CAPS_SCAN,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        SendInput(
            &[key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)],
            size_of::<INPUT>() as i32,
        )
    };
    HELD.store(sent == 2 && register_caps(), Relaxed);
    sent == 2
}

/// What the pill says about Caps Lock now.
pub fn caps_text() -> String {
    format!("Caps Lock {}", if caps_on() { "on" } else { "off" })
}

/// The Caps Lock key and the foreground layout, watched without seeing any other key.
pub struct Keys {
    hook: HWINEVENTHOOK,
    /// The foreground layout last reported; the first read only records it.
    layout: Option<isize>,
}

impl Keys {
    /// Hears foreground changes and takes the key. A refusal (no hook, or another program owns Caps Lock) only
    /// loses that half.
    pub fn new() -> Self {
        // SAFETY: a scalar thread id; the hook is out-of-context (no DLL is injected, no input is intercepted) and
        // delivered on this thread's message loop; a refused hook comes back null.
        let hook = unsafe {
            HOST.store(GetCurrentThreadId(), Relaxed);
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        grip();
        Self { hook, layout: None }
    }

    /// Whether `WM_HOTKEY`'s id is the Caps Lock key.
    pub fn is_caps(id: usize) -> bool {
        id == CAPS as usize
    }

    /// The foreground window changed: re-grips Caps Lock for the new foreground window, and returns the language's name
    /// when the foreground layout is not the one last reported (the first read only records it).
    pub fn looked(&mut self) -> Option<String> {
        LOOKING.store(false, Relaxed);
        grip();
        // SAFETY: scalar queries of the foreground window's thread and its layout.
        let layout = unsafe {
            let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            GetKeyboardLayout(thread).0 as isize
        };
        let changed = self
            .layout
            .replace(layout)
            .is_some_and(|before| before & 0xFFFF != layout & 0xFFFF);
        changed.then(|| language_name(layout))
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        if !self.hook.0.is_null() {
            // SAFETY: the hook this struct registered.
            let _ = unsafe { UnhookWinEvent(self.hook) };
        }
        if HELD.load(Relaxed) {
            // SAFETY: the hotkey this struct registered.
            let _ = unsafe { UnregisterHotKey(crate::native_window::wake_window(), CAPS) };
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_is_named_in_its_own_language() {
        assert_eq!(language_name(0x0409), "English");
        assert_eq!(
            language_name(0x0409_0409),
            "English",
            "the device word is ignored"
        );
        let arabic = language_name(0x0C01);
        assert!(
            arabic.chars().all(|c| ('\u{600}'..='\u{6FF}').contains(&c)),
            "{arabic}"
        );
    }

    #[test]
    fn the_caps_id_is_not_a_shortcut_group_id() {
        assert!((0..crate::hotkeys::COUNT).all(|group| (0..9).all(|index| {
            crate::hotkeys::action(CAPS as usize).is_none() && group * 16 + index != CAPS as usize
        })));
        assert!(Keys::is_caps(CAPS as usize) && !Keys::is_caps(0x20));
    }
}
