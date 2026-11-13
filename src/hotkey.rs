//! Global hotkeys (features plan slice 1): opt-in, non-hook system hotkeys via `RegisterHotKey`.
//! Zero polling, zero background threads; Windows posts `WM_HOTKEY` to the host thread message queue.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey,
    UnregisterHotKey, VK_SPACE,
};

pub const HOTKEY_PICKER_ID: i32 = 0x5101;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HotkeyCombo {
    #[default]
    AltSpace,
    CtrlSpace,
    CtrlShiftSpace,
    AltZ,
    CtrlAltSpace,
    AltP,
}

impl HotkeyCombo {
    pub const ALL: [Self; 6] = [
        Self::AltSpace,
        Self::CtrlSpace,
        Self::CtrlShiftSpace,
        Self::AltZ,
        Self::CtrlAltSpace,
        Self::AltP,
    ];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::AltSpace => "Alt + Space",
            Self::CtrlSpace => "Ctrl + Space",
            Self::CtrlShiftSpace => "Ctrl + Shift + Space",
            Self::AltZ => "Alt + Z",
            Self::CtrlAltSpace => "Ctrl + Alt + Space",
            Self::AltP => "Alt + P",
        }
    }

    pub const fn key(&self) -> &'static str {
        match self {
            Self::AltSpace => "alt_space",
            Self::CtrlSpace => "ctrl_space",
            Self::CtrlShiftSpace => "ctrl_shift_space",
            Self::AltZ => "alt_z",
            Self::CtrlAltSpace => "ctrl_alt_space",
            Self::AltP => "alt_p",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.key() == key)
    }

    pub fn modifiers_and_vk(&self) -> (HOT_KEY_MODIFIERS, u32) {
        match self {
            Self::AltSpace => (MOD_ALT | MOD_NOREPEAT, VK_SPACE.0 as u32),
            Self::CtrlSpace => (MOD_CONTROL | MOD_NOREPEAT, VK_SPACE.0 as u32),
            Self::CtrlShiftSpace => (MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, VK_SPACE.0 as u32),
            Self::AltZ => (MOD_ALT | MOD_NOREPEAT, 0x5A), // 'Z'
            Self::CtrlAltSpace => (MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_SPACE.0 as u32),
            Self::AltP => (MOD_ALT | MOD_NOREPEAT, 0x50), // 'P'
        }
    }
}

/// Registers the global picker hotkey on the calling thread's message queue.
/// Unregisters any previous registration under `HOTKEY_PICKER_ID` first.
pub fn register_picker(combo: HotkeyCombo) -> bool {
    unregister_picker();
    let (mods, vk) = combo.modifiers_and_vk();
    // SAFETY: RegisterHotKey called with None HWND associates the hotkey with the calling thread's queue.
    unsafe { RegisterHotKey(None::<HWND>, HOTKEY_PICKER_ID, mods, vk).is_ok() }
}

/// Unregisters the global picker hotkey from the calling thread's message queue.
pub fn unregister_picker() {
    // SAFETY: Unregistering an unassigned or assigned thread hotkey is safe and idempotent.
    let _ = unsafe { UnregisterHotKey(None::<HWND>, HOTKEY_PICKER_ID) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_combos_roundtrip_keys() {
        for combo in HotkeyCombo::ALL {
            assert_eq!(HotkeyCombo::from_key(combo.key()), Some(combo));
            assert!(!combo.label().is_empty());
        }
        assert_eq!(HotkeyCombo::from_key("invalid"), None);
    }

    #[test]
    fn modifiers_have_norepeat() {
        for combo in HotkeyCombo::ALL {
            let (mods, vk) = combo.modifiers_and_vk();
            assert!((mods.0 & MOD_NOREPEAT.0) != 0);
            assert!(vk > 0);
        }
    }
}
