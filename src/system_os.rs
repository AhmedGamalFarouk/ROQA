//! Small Windows switches the settings and quick settings reach: start at sign-in, keep awake, lock and sleep. Each is one
//! call made when the user asks; nothing here runs on its own.

use windows::{
    Win32::{
        Foundation::ERROR_SUCCESS,
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
    core::w,
};

/// Writes (on) or removes (off) ROQA's value under the user's Run key. Off by default and never touched
/// unless the switch changes, so a user who never turns it on gets no registry entry.
pub fn set_autostart(on: bool) {
    let Ok(exe) = std::env::current_exe() else {
        return;
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
            return;
        }
        if on {
            let _ = RegSetValueExW(key, w!("ROQA"), None, REG_SZ, Some(&data));
        } else {
            let _ = RegDeleteValueW(key, w!("ROQA"));
        }
        let _ = RegCloseKey(key);
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
