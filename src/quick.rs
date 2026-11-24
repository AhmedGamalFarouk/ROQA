//! Quick settings (the dock's sliders button): what Windows lets ROQA read and flip without a new
//! dependency. Wi-Fi and Bluetooth are radios (`Windows.Devices.Radios`), the power mode is the active
//! power overlay. Volume lives in `audio`. Brightness is not here: monitors answer over DDC/CI on the edge
//! slider's worker, which this popup cannot reach.
use std::{
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
use windows::{
    Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState},
    core::{GUID, Result},
};
use windows_future::{AsyncStatus, IAsyncOperation};

/// How long a radio call may take before the worker gives up on it. Local radio calls answer in a few
/// milliseconds; a stuck driver must not hold the worker for ever.
mod airplane;
pub mod networks;

const WAIT: Duration = Duration::from_millis(700);

fn wait<T: windows::core::RuntimeType + 'static>(operation: &IAsyncOperation<T>) -> Result<T> {
    let end = Instant::now() + WAIT;
    while operation.Status()? == AsyncStatus::Started {
        if Instant::now() >= end {
            let _ = operation.Cancel();
            return Err(windows::core::Error::from(
                windows::Win32::Foundation::ERROR_TIMEOUT.to_hresult(),
            ));
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "bounded wait on a WinRT call, on the radio worker (`refresh`), never the host"
        )]
        std::thread::sleep(Duration::from_millis(2));
    }
    operation.GetResults()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    WiFi,
    Bluetooth,
    Airplane,
}
impl Kind {
    fn radio(self) -> RadioKind {
        match self {
            Kind::WiFi => RadioKind::WiFi,
            Kind::Bluetooth => RadioKind::Bluetooth,
            Kind::Airplane => unreachable!(),
        }
    }
}

/// The radios as the last worker read them, `[Wi-Fi, Bluetooth]`; `None` while unknown or when the machine has no
/// such radio. Reading it is instant: the lookup and the flip are WinRT calls a worker waits on, never the host.
static LAST: Mutex<[Option<bool>; 2]> = Mutex::new([None; 2]);
static BUSY: AtomicBool = AtomicBool::new(false);
static CHANGED: AtomicBool = AtomicBool::new(false);
/// Posted to the host when a worker has read the radios; it only wakes the loop (`changed` says why).
const READY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 65;

/// Whether the radio was on at the last read; `None` when unknown or the machine has no such radio.
pub fn radio_on(kind: Kind) -> Option<bool> {
    if kind == Kind::Airplane {
        return airplane::state();
    }
    LAST.lock().unwrap_or_else(PoisonError::into_inner)[kind as usize]
}

/// Whether a worker stored new radio states since the last call.
pub fn airplane_fallback() -> bool {
    airplane::fallback()
}

pub fn changed() -> bool {
    CHANGED.swap(false, Ordering::AcqRel)
}

/// A flip asked while a run was out, waiting for it: 0 none, else the `Kind` plus one. The first waits; a second is
/// refused rather than replacing it. The host runs it when the run in flight reports (`run_waiting`).
static PENDING: AtomicU8 = AtomicU8::new(0);

fn take_pending() -> Option<Kind> {
    match PENDING.swap(0, Ordering::SeqCst) {
        1 => Some(Kind::WiFi),
        2 => Some(Kind::Bluetooth),
        3 => Some(Kind::Airplane),
        _ => None,
    }
}

/// Starts the flip that waited for the run which just reported (the host calls it when `changed`). The worker frees
/// its slot before it reports, and only the host thread asks, so the slot is free and no flip is lost between them.
pub fn run_waiting() {
    if let Some(kind) = take_pending() {
        refresh(Some(kind));
    }
}

/// Reads every radio and, with `flip`, switches that one first, all on a short worker. One run at a time: a flip
/// asked meanwhile waits for `run_waiting` (`PENDING`); false when a flip is already waiting or no worker could
/// start. The result comes with `READY` and `changed`.
pub fn refresh(flip: Option<Kind>) -> bool {
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    /// Frees the slot and balances the worker's COM, also when the worker unwinds.
    struct Done(bool);
    impl Drop for Done {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: balances the successful initialisation on this same worker.
                unsafe { CoUninitialize() };
            }
            BUSY.store(false, Ordering::SeqCst);
        }
    }
    if BUSY.swap(true, Ordering::SeqCst) {
        let Some(kind) = flip else { return false };
        return PENDING
            .compare_exchange(0, kind as u8 + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok();
    }
    // SAFETY: a scalar thread id query.
    let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let spawned = std::thread::Builder::new()
        .name("roqa-radio".into())
        .spawn(move || {
            // SAFETY: a new worker; the guard below balances it on this thread.
            let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            let done = Done(com);
            airplane::refresh(flip == Some(Kind::Airplane));
            match read(flip) {
                Ok(states) => *LAST.lock().unwrap_or_else(PoisonError::into_inner) = states,
                Err(error) => eprintln!("Quick settings: radio: {error}"),
            }
            drop(done);
            CHANGED.store(true, Ordering::Release);
            let _ = crate::native_window::post_wake(host, READY);
        });
    if spawned.is_err() {
        BUSY.store(false, Ordering::SeqCst);
    }
    spawned.is_ok()
}

/// The radios' states, after flipping `flip` when Windows allows it.
fn read(flip: Option<Kind>) -> Result<[Option<bool>; 2]> {
    let radios: Vec<Radio> = wait(&Radio::GetRadiosAsync()?)?.into_iter().collect();
    let mut states = [None; 2];
    for kind in [Kind::WiFi, Kind::Bluetooth] {
        let Some(radio) = radios.iter().find(|radio| radio.Kind() == Ok(kind.radio())) else {
            continue;
        };
        if flip == Some(kind) && wait(&Radio::RequestAccessAsync()?)? == RadioAccessStatus::Allowed
        {
            let to = if radio.State()? == RadioState::On {
                RadioState::Off
            } else {
                RadioState::On
            };
            wait(&radio.SetStateAsync(to)?)?;
        }
        states[kind as usize] = radio.State().ok().map(|state| state == RadioState::On);
    }
    Ok(states)
}

/// The power overlays of Settings > System > Power, from the most thrifty to the fastest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Efficiency,
    Balanced,
    Performance,
    Best,
}
impl Mode {
    const ALL: [Mode; 4] = [
        Mode::Efficiency,
        Mode::Balanced,
        Mode::Performance,
        Mode::Best,
    ];
    const fn overlay(self) -> GUID {
        GUID::from_u128(match self {
            Mode::Efficiency => 0x961cc777_2547_4f9d_8174_7d86181b8a7a,
            Mode::Balanced => 0,
            Mode::Performance => 0x3af9b8d9_7c97_431d_ad78_34a8bfea439f,
            Mode::Best => 0xded574b5_45a0_4f42_8737_46345c09c238,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Mode::Efficiency => "Best power efficiency",
            Mode::Balanced => "Balanced power",
            Mode::Performance => "Better performance",
            Mode::Best => "Best performance",
        }
    }
}

#[link(name = "powrprof", kind = "raw-dylib")]
unsafe extern "system" {
    fn PowerGetEffectiveOverlayScheme(scheme: *mut GUID) -> u32;
    fn PowerSetActiveOverlayScheme(scheme: GUID) -> u32;
}

/// The active power mode, `None` where Windows has no power overlays (before Windows 10 1709).
pub fn mode() -> Option<Mode> {
    let mut scheme = GUID::zeroed();
    // SAFETY: a writable GUID for the call to fill.
    let status = unsafe { PowerGetEffectiveOverlayScheme(&mut scheme) };
    (status == 0)
        .then(|| Mode::ALL.into_iter().find(|mode| mode.overlay() == scheme))
        .flatten()
}

/// Steps to the next power mode, wrapping from the fastest back to the most thrifty.
pub fn next_mode() {
    let Some(now) = mode() else { return };
    let at = Mode::ALL.iter().position(|mode| *mode == now).unwrap_or(0);
    let next = Mode::ALL[(at + 1) % Mode::ALL.len()];
    // SAFETY: a scalar overlay id passed by value.
    let status = unsafe { PowerSetActiveOverlayScheme(next.overlay()) };
    if status != 0 {
        eprintln!("Quick settings: power mode: error {status}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_radio_rows_read_the_last_states() {
        *LAST.lock().unwrap() = [Some(true), None];
        assert_eq!(radio_on(Kind::WiFi), Some(true));
        assert_eq!(radio_on(Kind::Bluetooth), None);
        *LAST.lock().unwrap() = [None; 2];
    }

    #[test]
    fn a_flip_asked_during_a_run_waits_and_a_second_is_refused() {
        BUSY.store(true, Ordering::SeqCst); // a run is out; no worker starts below
        assert!(!refresh(None), "a plain read just yields");
        assert!(refresh(Some(Kind::WiFi)), "the first flip waits");
        assert!(
            !refresh(Some(Kind::Bluetooth)),
            "the first is kept, not replaced"
        );
        assert_eq!(take_pending(), Some(Kind::WiFi));
        assert_eq!(take_pending(), None);
        BUSY.store(false, Ordering::SeqCst);
    }

    #[test]
    fn each_mode_has_its_own_overlay_and_label() {
        for (at, a) in Mode::ALL.iter().enumerate() {
            for b in &Mode::ALL[..at] {
                assert!(a.overlay() != b.overlay() && a.label() != b.label());
            }
        }
    }
}
