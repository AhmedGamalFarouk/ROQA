//! Global shortcuts. Each key is one `RegisterHotKey` on the host thread, so `WM_HOTKEY` arrives in the loop
//! that already runs: no hook, no polling. Windows keeps Win+number and Win+B for its own taskbar (a
//! registration of them fails on this PC), so these use Ctrl+Alt.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};

/// The shortcut groups: the notch, the dock's apps, the dock itself and the hidden-icons flyout.
pub const COUNT: usize = 4;
/// Setting keys, written `hk<key>`.
pub const KEYS: [&str; COUNT] = ["notch", "apps", "dock", "tray"];
/// The keys of each group: apps are 1 to 9, the others one letter.
const LETTERS: [&[u8]; COUNT] = [b"N", b"123456789", b"D", b"B"];

/// The id `WM_HOTKEY` carries: the group (from 1) over the key's place in the group.
fn id(group: usize, index: usize) -> i32 {
    (((group + 1) << 4) | index) as i32
}

/// The group and key place a `WM_HOTKEY` id stands for, `None` for an id that is not ours.
pub fn action(id: usize) -> Option<(usize, usize)> {
    let group = (id >> 4).checked_sub(1)?;
    (group < COUNT && (id & 15) < LETTERS[group].len()).then_some((group, id & 15))
}

fn combo(key: u8) -> String {
    format!("Ctrl+Alt+{}", key as char)
}

#[derive(Default)]
pub struct Hotkeys {
    /// Groups registered (or tried) now.
    on: [bool; COUNT],
    /// The first key of a group that Windows refused: another app owns it.
    clash: [Option<u8>; COUNT],
}

impl Hotkeys {
    /// Registers the groups newly wanted and releases the ones no longer wanted.
    pub fn sync(&mut self, want: [bool; COUNT]) {
        for group in 0..COUNT {
            if want[group] == self.on[group] {
                continue;
            }
            (self.on[group], self.clash[group]) = (want[group], None);
            for (index, &key) in LETTERS[group].iter().enumerate() {
                // SAFETY: a thread-bound hotkey (no window) with scalar arguments.
                let done = unsafe {
                    if want[group] {
                        RegisterHotKey(
                            None,
                            id(group, index),
                            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
                            u32::from(key),
                        )
                    } else {
                        UnregisterHotKey(None, id(group, index))
                    }
                };
                if done.is_err() && want[group] {
                    self.clash[group].get_or_insert(key);
                }
            }
        }
    }

    /// Each row's caption: its key combination, or in plain words which one another app already holds.
    pub fn notes(&self) -> [String; COUNT] {
        std::array::from_fn(|group| {
            let keys = LETTERS[group];
            match self.clash[group] {
                Some(key) => format!("{} is taken by another app.", combo(key)),
                None if keys.len() > 1 => {
                    format!("{} to {}", combo(keys[0]), keys[keys.len() - 1] as char)
                }
                None => combo(keys[0]),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_one_id_that_reads_back() {
        let mut seen = Vec::new();
        for (group, keys) in LETTERS.iter().enumerate() {
            for index in 0..keys.len() {
                let id = id(group, index);
                assert_eq!(action(id as usize), Some((group, index)));
                assert!(!seen.contains(&id));
                seen.push(id);
            }
        }
        assert_eq!(
            (action(0), action(0x10 + 9), action(0x50)),
            (None, None, None)
        );
    }

    #[test]
    fn a_row_shows_its_keys_or_the_clash() {
        let mut keys = Hotkeys::default();
        assert_eq!(keys.notes()[0], "Ctrl+Alt+N");
        assert_eq!(keys.notes()[1], "Ctrl+Alt+1 to 9");
        keys.clash[1] = Some(b'3');
        assert_eq!(keys.notes()[1], "Ctrl+Alt+3 is taken by another app.");
    }
}
