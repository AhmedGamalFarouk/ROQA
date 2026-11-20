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

/// The one eject slot: 0 free, `BUSY` while a worker runs, otherwise a finished answer waiting to be read
/// (`letter << 8 | 2` refused, `| 3` safe to remove). A new eject starts only from 0, so an unread answer is never
/// replaced and each answer names its drive.
static EJECT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
const BUSY: u32 = 1;
/// Posted to the host thread when an eject finished; it only wakes the loop, `ejected` carries the answer.
const EJECTED: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 61;

fn eject_begin() -> bool {
    use std::sync::atomic::Ordering::{AcqRel, Acquire};
    EJECT.compare_exchange(0, BUSY, AcqRel, Acquire).is_ok()
}

fn eject_finish(letter: char, safe: bool) {
    EJECT.store(
        (letter as u32) << 8 | if safe { 3 } else { 2 },
        std::sync::atomic::Ordering::Release,
    );
}

/// `eject` on a short worker: locking and dismounting flush the drive, which can take seconds on a slow stick and
/// must not stop the dock and notch. The answer comes back through `ejected`. False, and nothing starts, while an
/// earlier eject is out or its answer is unread.
pub fn eject_in_background(letter: char) -> bool {
    if !eject_begin() {
        return false;
    }
    // SAFETY: a scalar thread id query.
    let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let spawned = std::thread::Builder::new()
        .name("roqa-eject".into())
        .spawn(move || {
            eject_finish(letter, eject(letter));
            // A failed post is picked up on the host's next pass.
            let _ = crate::native_window::post_wake(host, EJECTED);
        });
    if spawned.is_err() {
        eject_finish(letter, false);
    }
    true
}

/// The finished eject's drive and answer, once: `true` when the drive is safe to remove.
pub fn ejected() -> Option<(char, bool)> {
    use std::sync::atomic::Ordering::{AcqRel, Acquire};
    let done = EJECT
        .fetch_update(AcqRel, Acquire, |state| (state > BUSY).then_some(0))
        .ok()?;
    Some((char::from_u32(done >> 8)?, done & 0xFF == 3))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_eject_at_a_time_keeps_the_first_unread_answer_and_names_its_drive() {
        assert_eq!(ejected(), None);
        assert!(eject_begin());
        assert!(!eject_begin(), "busy while the worker runs");
        assert_eq!(ejected(), None, "nothing to read yet");
        eject_finish('E', true);
        assert!(!eject_begin(), "an unread answer is never replaced");
        assert_eq!(ejected(), Some(('E', true)));
        assert_eq!(ejected(), None, "read once");
        assert!(eject_begin());
        eject_finish('F', false);
        assert_eq!(ejected(), Some(('F', false)));
    }
}
