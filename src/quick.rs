//! Quick settings (the dock's sliders button): what Windows lets ROQA read and flip without a new
//! dependency. Wi-Fi and Bluetooth are radios (`Windows.Devices.Radios`), the power mode is the active
//! power overlay. Volume lives in `audio`. Brightness is not here: monitors answer over DDC/CI on the edge
//! slider's worker, which this popup cannot reach.
use std::time::{Duration, Instant};
use windows::{
    Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState},
    core::{GUID, Result},
};
use windows_future::{AsyncStatus, IAsyncOperation};

/// How long a radio call may take before the popup gives up on it. Local radio calls answer in a few
/// milliseconds; a stuck driver must not hold the host thread.
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
            reason = "bounded wait on a WinRT call, on a worker"
        )]
        std::thread::sleep(Duration::from_millis(2));
    }
    operation.GetResults()
}

thread_local! {
    /// The radios found so far (UI thread only): reading `State()` of a kept one is instant, while the lookup
    /// waits on the host thread.
    static RADIOS: std::cell::RefCell<Vec<Radio>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The radio of `kind`, when the machine has one; looked up again only when the kept one stopped answering.
fn radio(kind: RadioKind) -> Option<Radio> {
    let kept = RADIOS.with_borrow(|radios| {
        radios
            .iter()
            .find(|radio| radio.Kind() == Ok(kind) && radio.State().is_ok())
            .cloned()
    });
    if kept.is_some() {
        return kept;
    }
    let found = wait(&Radio::GetRadiosAsync().ok()?).ok()?;
    let radios: Vec<Radio> = found.into_iter().collect();
    let radio = radios
        .iter()
        .find(|radio| radio.Kind() == Ok(kind))
        .cloned();
    RADIOS.set(radios);
    radio
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    WiFi,
    Bluetooth,
}
impl Kind {
    fn radio(self) -> RadioKind {
        match self {
            Kind::WiFi => RadioKind::WiFi,
            Kind::Bluetooth => RadioKind::Bluetooth,
        }
    }
}

/// Whether the radio is on; `None` when the machine has no such radio (or it cannot be read).
pub fn radio_on(kind: Kind) -> Option<bool> {
    Some(radio(kind.radio())?.State().ok()? == RadioState::On)
}

/// Flips the radio. Returns whether Windows accepted it.
pub fn toggle_radio(kind: Kind) -> bool {
    let flip = || -> Result<bool> {
        let Some(radio) = radio(kind.radio()) else {
            return Ok(false);
        };
        if wait(&Radio::RequestAccessAsync()?)? != RadioAccessStatus::Allowed {
            return Ok(false);
        }
        let to = if radio.State()? == RadioState::On {
            RadioState::Off
        } else {
            RadioState::On
        };
        Ok(wait(&radio.SetStateAsync(to)?)? == RadioAccessStatus::Allowed)
    };
    flip().unwrap_or_else(|error| {
        eprintln!("Quick settings: radio: {error}");
        false
    })
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
    fn each_mode_has_its_own_overlay_and_label() {
        for (at, a) in Mode::ALL.iter().enumerate() {
            for b in &Mode::ALL[..at] {
                assert!(a.overlay() != b.overlay() && a.label() != b.label());
            }
        }
    }
}
