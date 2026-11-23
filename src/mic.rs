//! Microphone mute: the default recording device's own mute, for the notch's muted dot, a click on the mic dot and
//! Ctrl+Alt+M. One endpoint-volume callback and one device callback; like `audio.rs`, they only set a bit and post
//! one wake, and every bind, read and write happens on the host thread. No polling.

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use windows::{
    Win32::{
        Foundation::PROPERTYKEY,
        Media::Audio::{
            AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole,
            Endpoints::{
                IAudioEndpointVolume, IAudioEndpointVolumeCallback,
                IAudioEndpointVolumeCallback_Impl,
            },
            IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
            MMDeviceEnumerator, eCapture, eConsole,
        },
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance},
            Threading::GetCurrentThreadId,
        },
        UI::WindowsAndMessaging::WM_APP,
    },
    core::{PCWSTR, Result, implement},
};

/// Thread message that only wakes the host loop; the work waits in `Wake::dirty`.
const WAKE: u32 = WM_APP + 55;
/// The default recording device may have changed: bind it again.
const REBIND: u32 = 1;
/// The bound device's mute may have changed: read it again.
const MUTE: u32 = 2;

struct Wake {
    dirty: AtomicU32,
    host: u32,
}
impl Wake {
    fn raise(&self, bit: u32) {
        if self.dirty.fetch_or(bit, Ordering::Relaxed) == 0 {
            let _ = crate::native_window::post_wake(self.host, WAKE);
        }
    }
}

#[implement(IMMNotificationClient)]
struct DeviceEvents(Arc<Wake>);
impl IMMNotificationClient_Impl for DeviceEvents_Impl {
    // Only a new default recording device matters; one that is removed is followed by this event.
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> Result<()> {
        if flow == eCapture && role == eConsole {
            self.0.raise(REBIND);
        }
        Ok(())
    }
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct MuteEvents(Arc<Wake>);
impl IAudioEndpointVolumeCallback_Impl for MuteEvents_Impl {
    fn OnNotify(&self, _: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        self.0.raise(MUTE);
        Ok(())
    }
}

/// The bound device's volume and its callback; dropping it unregisters, then releases.
struct Endpoint {
    volume: IAudioEndpointVolume,
    events: IAudioEndpointVolumeCallback,
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        // SAFETY: host thread, outside any callback; `events` is the callback registered on it.
        let _ = unsafe { self.volume.UnregisterControlChangeNotify(&self.events) };
    }
}

/// Host-thread owner of the mic's Core Audio objects. Field order is teardown order: the endpoint first.
pub struct Mic {
    endpoint: Option<Endpoint>,
    devices: Option<(IMMDeviceEnumerator, IMMNotificationClient)>,
    muted: bool,
    wake: Arc<Wake>,
}
impl Drop for Mic {
    fn drop(&mut self) {
        self.endpoint = None;
        if let Some((enumerator, events)) = &self.devices {
            // SAFETY: host thread; `events` is the client registered on this enumerator.
            let _ = unsafe { enumerator.UnregisterEndpointNotificationCallback(events) };
        }
    }
}
impl Mic {
    /// Subscribes once on the host (STA) thread; the next `refresh` binds the default recording device. Without
    /// Core Audio there is no mic: nothing shows and a toggle says so.
    pub fn start() -> Self {
        let wake = Arc::new(Wake {
            dirty: AtomicU32::new(REBIND),
            // SAFETY: reads this thread's ID; no pointers.
            host: unsafe { GetCurrentThreadId() },
        });
        let events: IMMNotificationClient = DeviceEvents(Arc::clone(&wake)).into();
        // SAFETY: host STA thread; `events` stays alive in `devices` until it is unregistered.
        let devices = unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).and_then(
                |enumerator: IMMDeviceEnumerator| {
                    enumerator.RegisterEndpointNotificationCallback(&events)?;
                    Ok((enumerator, events))
                },
            )
        }
        .inspect_err(|error| eprintln!("Mic: device events: {error}"))
        .ok();
        Self {
            endpoint: None,
            devices,
            muted: false,
            wake,
        }
    }
    /// Applies pending callbacks; true when the mute changed. Without any it costs one atomic swap.
    pub fn refresh(&mut self) -> bool {
        let dirty = self.wake.dirty.swap(0, Ordering::Relaxed);
        if dirty == 0 {
            return false;
        }
        if dirty & REBIND != 0 {
            self.endpoint = None;
            self.endpoint = self.devices.as_ref().and_then(|(enumerator, _)| {
                bind(enumerator, &self.wake)
                    .inspect_err(|error| eprintln!("Mic: no recording device: {error}"))
                    .ok()
            });
        }
        let before = self.muted;
        self.muted = self.endpoint.as_ref().is_some_and(|endpoint| {
            // SAFETY: host-thread read on the bound endpoint.
            unsafe { endpoint.volume.GetMute() }.is_ok_and(|m| m.as_bool())
        });
        before != self.muted
    }
    pub fn muted(&self) -> bool {
        self.muted
    }
    /// Mutes or unmutes the default recording device; the callback brings the new state to `refresh`. `None`
    /// without a device, else whether it is muted now.
    pub fn toggle(&mut self) -> Option<bool> {
        let endpoint = self.endpoint.as_ref()?;
        let muted = !self.muted;
        // SAFETY: host-thread write on the bound endpoint; no event context, so the callback wakes as for any change.
        unsafe { endpoint.volume.SetMute(muted, std::ptr::null()) }
            .inspect_err(|error| eprintln!("Mic: mute: {error}"))
            .ok()?;
        Some(muted)
    }
}

fn bind(enumerator: &IMMDeviceEnumerator, wake: &Arc<Wake>) -> Result<Endpoint> {
    let events: IAudioEndpointVolumeCallback = MuteEvents(Arc::clone(wake)).into();
    // SAFETY: host-thread calls on live interfaces; `events` stays alive in `Endpoint` until it is unregistered.
    unsafe {
        let device = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?;
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
        volume.RegisterControlChangeNotify(&events)?;
        Ok(Endpoint { volume, events })
    }
}

/// The pill a toggle shows: what the mic is now.
pub fn toggle_text(muted: Option<bool>) -> &'static str {
    match muted {
        Some(true) => "Mic muted",
        Some(false) => "Mic on",
        None => "No microphone found",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::Audio::{eCommunications, eRender};

    #[test]
    fn only_a_new_default_recording_device_rebinds_and_any_mute_change_rereads() {
        let wake = Arc::new(Wake {
            dirty: AtomicU32::new(0),
            host: 0,
        });
        let client: IMMNotificationClient = DeviceEvents(Arc::clone(&wake)).into();
        let callback: IAudioEndpointVolumeCallback = MuteEvents(Arc::clone(&wake)).into();
        let take = || wake.dirty.swap(0, Ordering::Relaxed);
        // SAFETY: our own callback objects, given a null ID and null data.
        unsafe {
            for (flow, role) in [(eRender, eConsole), (eCapture, eCommunications)] {
                client
                    .OnDefaultDeviceChanged(flow, role, PCWSTR::null())
                    .unwrap();
                assert_eq!(take(), 0);
            }
            client
                .OnDefaultDeviceChanged(eCapture, eConsole, PCWSTR::null())
                .unwrap();
            assert_eq!(take(), REBIND);
            callback.OnNotify(std::ptr::null_mut()).unwrap();
            assert_eq!(take(), MUTE);
        }
        assert_eq!(toggle_text(Some(true)), "Mic muted");
        assert_eq!(toggle_text(None), "No microphone found");
    }
}
