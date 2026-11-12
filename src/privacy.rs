//! Privacy indicator (features B5): an orange dot while any app uses the microphone, a green one for the
//! camera. Windows records it per app in the user's consent store (`LastUsedTimeStart` set and
//! `LastUsedTimeStop` 0 means in use). One `RegNotifyChangeKeyValue` handle sits in the event loop's wait set:
//! no thread, no polling, one registry read at start and one per signal. App names are never kept or logged.

use crate::ipc::OwnedHandle;
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::{ERROR_SUCCESS, HANDLE},
        System::{
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET,
                RRF_RT_REG_QWORD, RegCloseKey, RegEnumKeyExW, RegGetValueW,
                RegNotifyChangeKeyValue, RegOpenKeyExW,
            },
            Threading::CreateEventW,
        },
    },
    core::{PCWSTR, PWSTR, w},
};

/// The consent store's own values: started and not yet stopped.
pub fn in_use(start: u64, stop: u64) -> bool {
    start != 0 && stop == 0
}

/// Which devices are in use right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Use {
    pub mic: bool,
    pub cam: bool,
}

const LEVELS: u8 = 3;
const STEP: Duration = Duration::from_millis(60);

/// The dots' fade: each dot's level climbs or falls one step per 60 ms (3 steps), only while it is changing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Dots {
    target: Use,
    level: [u8; 2],
    next: Option<Instant>,
}

impl Dots {
    /// Sets what is in use; true when the dots need a repaint now.
    pub fn set(&mut self, target: Use, now: Instant) -> bool {
        if target == self.target {
            return false;
        }
        self.target = target;
        self.next = Some(now);
        self.tick(now)
    }
    /// Steps the fade when due; true when the drawn levels changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.next.is_none_or(|next| now < next) {
            return false;
        }
        let goal = [self.target.mic, self.target.cam].map(|on| if on { LEVELS } else { 0 });
        for (level, goal) in self.level.iter_mut().zip(goal) {
            if *level < goal {
                *level += 1;
            } else if *level > goal {
                *level -= 1;
            }
        }
        self.next = (self.level != goal).then_some(now + STEP);
        true
    }
    pub fn due(&self) -> Option<Instant> {
        self.next
    }
    /// Opacity of the microphone and camera dots, 0 to 1.
    pub fn alpha(&self) -> [f32; 2] {
        self.level.map(|level| f32::from(level) / f32::from(LEVELS))
    }
}

/// The registry watch: the consent store key, and the event it signals on any change below it.
pub struct Watch {
    key: HKEY,
    event: OwnedHandle,
}

const STORE: PCWSTR =
    w!(r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore");

impl Watch {
    /// `None` when the key cannot be opened (the indicator then simply never shows).
    pub fn new() -> Option<Self> {
        // SAFETY: valid terminated key name and out pointer; the event is auto-reset and unnamed.
        unsafe {
            let mut key = HKEY::default();
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                STORE,
                None,
                KEY_READ | KEY_NOTIFY,
                &mut key,
            )
            .is_ok()
            .then_some(())?;
            let Ok(event) = CreateEventW(None, false, false, None) else {
                let _ = RegCloseKey(key);
                return None;
            };
            let watch = Self {
                key,
                event: OwnedHandle(event),
            };
            watch.arm();
            Some(watch)
        }
    }
    pub fn handle(&self) -> HANDLE {
        self.event.0
    }
    /// Asks for the next change; the registry needs this again after every signal.
    pub fn arm(&self) {
        // SAFETY: the key and event are alive for as long as self.
        unsafe {
            let _ = RegNotifyChangeKeyValue(
                self.key,
                true,
                REG_NOTIFY_CHANGE_LAST_SET,
                Some(self.event.0),
                true,
            );
        }
    }
    /// Re-reads the store: whether any app has the microphone or the camera open.
    pub fn read(&self) -> Use {
        Use {
            mic: self.any(w!("microphone")),
            cam: self.any(w!("webcam")),
        }
    }
    /// Any app under `kind` and `kind\NonPackaged` in use.
    fn any(&self, kind: PCWSTR) -> bool {
        // SAFETY: handles opened and closed in this scope; buffers are as long as the sizes passed.
        unsafe {
            let mut root = HKEY::default();
            if RegOpenKeyExW(self.key, kind, None, KEY_READ, &mut root) != ERROR_SUCCESS {
                return false;
            }
            let mut nonpackaged = HKEY::default();
            let found = apps_in_use(root)
                || (RegOpenKeyExW(root, w!("NonPackaged"), None, KEY_READ, &mut nonpackaged)
                    == ERROR_SUCCESS
                    && apps_in_use(nonpackaged));
            if !nonpackaged.is_invalid() {
                let _ = RegCloseKey(nonpackaged);
            }
            let _ = RegCloseKey(root);
            found
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // SAFETY: the key was opened by `new` and is closed once.
        unsafe {
            let _ = RegCloseKey(self.key);
        }
    }
}

/// A value of `key` as a QWORD; 0 when missing.
unsafe fn qword(key: HKEY, name: PCWSTR) -> u64 {
    let (mut value, mut size) = (0u64, 8u32);
    // SAFETY: `value` is 8 bytes, as `size` says.
    unsafe {
        let _ = RegGetValueW(
            key,
            None,
            name,
            RRF_RT_REG_QWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&mut size),
        );
    }
    value
}

/// Walks the app subkeys of `parent` (at most 256) for one whose start is set and stop is 0.
unsafe fn apps_in_use(parent: HKEY) -> bool {
    let mut name = [0u16; 300];
    for index in 0..256 {
        let mut length = name.len() as u32;
        // SAFETY: `name` holds `length` units; the app key is closed before the next one.
        unsafe {
            if RegEnumKeyExW(
                parent,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut length,
                None,
                None,
                None,
                None,
            ) != ERROR_SUCCESS
            {
                return false;
            }
            let mut app = HKEY::default();
            if RegOpenKeyExW(parent, PCWSTR(name.as_ptr()), None, KEY_READ, &mut app)
                != ERROR_SUCCESS
            {
                continue;
            }
            let used = in_use(
                qword(app, w!("LastUsedTimeStart")),
                qword(app, w!("LastUsedTimeStop")),
            );
            let _ = RegCloseKey(app);
            if used {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_use_needs_a_start_and_a_zero_stop() {
        // Fixtures: never used, in use, stopped (FILETIME-sized values), and a stale stop of 0 with no start.
        assert!(!in_use(0, 0));
        assert!(in_use(133_800_000_000_000_000, 0));
        assert!(!in_use(133_800_000_000_000_000, 133_800_100_000_000_000));
        assert!(!in_use(0, 133_800_100_000_000_000));
    }

    #[test]
    fn dots_fade_in_and_out_in_three_steps_then_rest() {
        let t0 = Instant::now();
        let mut dots = Dots::default();
        assert!(dots.due().is_none() && !dots.tick(t0), "idle costs nothing");
        assert!(!dots.set(Use::default(), t0));
        assert!(dots.set(
            Use {
                mic: true,
                cam: false
            },
            t0
        ));
        assert_eq!(dots.alpha(), [1.0 / 3.0, 0.0]);
        let mut now = t0;
        while let Some(due) = dots.due() {
            now = due;
            assert!(dots.tick(now));
        }
        assert_eq!(dots.alpha(), [1.0, 0.0]);
        assert!(now - t0 <= STEP * 2, "three levels in two waits");
        assert!(dots.set(Use::default(), now));
        while let Some(due) = dots.due() {
            dots.tick(due);
        }
        assert_eq!(dots.alpha(), [0.0, 0.0]);
        assert!(dots.due().is_none());
    }
}
