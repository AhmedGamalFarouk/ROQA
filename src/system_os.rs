//! Small Windows switches the settings and quick settings reach: start at sign-in. Each is one call made
//! when the user asks; nothing here runs on its own.

use windows::{
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW,
            RegOpenKeyExW, RegSetValueExW,
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
