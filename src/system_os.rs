//! Small Windows switches the settings and quick settings reach: start at sign-in, keep awake, lock and sleep. Each is one
//! call made when the user asks; nothing here runs on its own.

use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE, HANDLE,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{
            Power::{
                ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetSuspendState,
                SetThreadExecutionState,
            },
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW,
                RegOpenKeyExW, RegSetValueExW,
            },
        },
    },
    core::{HSTRING, w},
};

/// Writes (on) or removes (off) ROQA's value under the user's Run key. Off by default and never touched
/// unless the switch changes, so a user who never turns it on gets no registry entry. False when the registry
/// refused, so the switch can say so and go back instead of promising a start that will not happen.
pub fn set_autostart(on: bool) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let command = format!("\"{}\"", exe.display());
    let data: Vec<u8> = command
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    // SAFETY: valid terminated key and value names, an out handle closed below, and a wide string
    // that includes its terminator.
    unsafe {
        let mut key = HKEY::default();
        let opened = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            None,
            KEY_SET_VALUE,
            &mut key,
        );
        if opened != ERROR_SUCCESS {
            return false;
        }
        let done = if on {
            RegSetValueExW(key, w!("ROQA"), None, REG_SZ, Some(&data)) == ERROR_SUCCESS
        } else {
            // Nothing to remove is as good as removed.
            matches!(
                RegDeleteValueW(key, w!("ROQA")),
                ERROR_SUCCESS | ERROR_FILE_NOT_FOUND
            )
        };
        let _ = RegCloseKey(key);
        done
    }
}

/// Keeps the screen and the PC awake while on. The request belongs to the calling (host) thread, so
/// Windows drops it when ROQA quits.
pub fn keep_awake(on: bool) {
    let flags = if on {
        ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    // SAFETY: a scalar flag call with no pointers.
    unsafe {
        SetThreadExecutionState(flags);
    }
}

#[link(name = "kernel32", kind = "raw-dylib")]
unsafe extern "system" {
    fn DeviceIoControl(
        device: HANDLE,
        code: u32,
        input: *const u8,
        input_len: u32,
        output: *mut u8,
        output_len: u32,
        returned: *mut u32,
        overlapped: *mut u8,
    ) -> i32;
}

/// Ejects drive `letter` the way Safe Remove does: lock (it fails while a file is open, so nothing is lost),
/// dismount, allow removal, eject. False when any step is refused.
pub fn eject(letter: char) -> bool {
    const LOCK: u32 = 0x0009_0018;
    const DISMOUNT: u32 = 0x0009_0020;
    const ALLOW_REMOVAL: u32 = 0x002D_4804;
    const EJECT: u32 = 0x002D_4808;
    let path = HSTRING::from(format!(r"\\.\{letter}:"));
    // SAFETY: a volume handle closed below; each call passes a live buffer of the length it states.
    unsafe {
        let Ok(volume) = CreateFileW(
            &path,
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        ) else {
            return false;
        };
        let mut returned = 0;
        let mut control = |code: u32, input: &[u8]| {
            DeviceIoControl(
                volume,
                code,
                input.as_ptr(),
                input.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            ) != 0
        };
        let done = control(LOCK, &[])
            && control(DISMOUNT, &[])
            && control(ALLOW_REMOVAL, &[0])
            && control(EJECT, &[]);
        let _ = CloseHandle(volume);
        done
    }
}

/// What the last `eject_in_background` found: 0 none yet, 1 safe to remove, 2 refused. Read once by `ejected`.
static EJECT_RESULT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// Posted to the host thread when an eject finished; it only wakes the loop, `ejected` carries the answer.
const EJECTED: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 61;

/// `eject` on a short worker: locking and dismounting flush the drive, which can take seconds on a slow stick and
/// must not stop the dock and notch. The answer comes back through `ejected`.
pub fn eject_in_background(letter: char) {
    // SAFETY: a scalar thread id query.
    let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let spawned = std::thread::Builder::new()
        .name("roqa-eject".into())
        .spawn(move || {
            let answer = if eject(letter) { 1 } else { 2 };
            EJECT_RESULT.store(answer, std::sync::atomic::Ordering::Release);
            // A failed post is picked up on the host's next pass.
            let _ = crate::native_window::post_wake(host, EJECTED);
        });
    if spawned.is_err() {
        EJECT_RESULT.store(2, std::sync::atomic::Ordering::Release);
    }
}

/// The finished eject's answer, once: `Some(true)` when the drive is safe to remove.
pub fn ejected() -> Option<bool> {
    match EJECT_RESULT.swap(0, std::sync::atomic::Ordering::AcqRel) {
        0 => None,
        answer => Some(answer == 1),
    }
}

#[link(name = "user32", kind = "raw-dylib")]
unsafe extern "system" {
    fn LockWorkStation() -> i32;
}

/// Locks the session, as Win+L does.
pub fn lock() {
    // SAFETY: no arguments; Windows refuses quietly when it cannot lock.
    unsafe {
        LockWorkStation();
    }
}

/// Puts the PC to sleep (not hibernate), asking running apps nothing.
pub fn sleep() {
    // SAFETY: scalar flags only.
    unsafe {
        let _ = SetSuspendState(false, false, false);
    }
}
