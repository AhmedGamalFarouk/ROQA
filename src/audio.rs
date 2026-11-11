//! The default render endpoint's master volume and mute, for the media panel's volume row, and its
//! peak meter, for the visualizer.
//!
//! Core Audio starts the first time a media notch is shown and stays bound until shutdown. Its
//! callbacks may run on any Core Audio thread: they only set a dirty bit and post one scalar wake,
//! so they never block, panic, register or unregister. Every bind, read and write happens on the
//! host (STA) thread.
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use windows::{
    Win32::{
        Foundation::{LPARAM, PROPERTYKEY, WPARAM},
        Media::Audio::{
            AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole,
            Endpoints::{
                IAudioEndpointVolume, IAudioEndpointVolumeCallback,
                IAudioEndpointVolumeCallback_Impl, IAudioMeterInformation,
            },
            IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
            MMDeviceEnumerator, eConsole, eRender,
        },
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance},
            Threading::GetCurrentThreadId,
        },
        UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
    },
    core::{GUID, PCWSTR, Result, implement},
};

/// The visualizer's bar count: one per meter sample, oldest first.
pub const BARS: usize = 5;
/// Index of the mute toggle, after the three media controls, in drawing, focus and UIA order.
pub const MUTE: usize = 3;
/// Thread message that only wakes the host loop; the pending work is in `Wake::dirty`.
const WAKE: u32 = WM_APP + 51;
/// The default endpoint may have changed: bind it again.
const REBIND: u32 = 1;
/// The bound endpoint's level or mute changed: read it again.
const VOLUME: u32 = 2;
/// Event context of ROQA's own changes. Their notifications wake nothing: the setter already
/// queued its own re-read.
const ECHO: GUID = GUID::from_u128(0x9d8f3897_b1df_4a66_add8_1c91eefc0f51);

/// What the volume row shows: the whole percent, as Windows shows it, and mute. The bar and the
/// accessible name are drawn from this alone, so a smaller change repaints nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level {
    pub percent: u8,
    pub muted: bool,
}

/// Shifts the oldest sample out and `peak`, clamped to 0..=1, in as the newest.
fn push(peaks: &mut [f32; BARS], peak: f32) {
    peaks.rotate_left(1);
    peaks[BARS - 1] = if peak.is_nan() {
        0.0
    } else {
        peak.clamp(0.0, 1.0)
    };
}

/// Whether setting the level from the notch also unmutes: a level the user picks is meant to be
/// heard (user decision, 2026-11-11).
fn unmutes(shown: Level) -> bool {
    shown.muted
}

fn percent(scalar: f32) -> u8 {
    (scalar.clamp(0.0, 1.0) * 100.0).round() as u8
}

/// The only state callbacks touch.
struct Wake {
    dirty: AtomicU32,
    host: u32,
}
impl Wake {
    /// Callback side; posts only when no wake is pending already. A lost post leaves the bit set
    /// for the loop's next finite deadline (the clock's, at most a minute).
    fn raise(&self, bit: u32) {
        if self.dirty.fetch_or(bit, Ordering::Relaxed) == 0 {
            // SAFETY: a scalar thread message with no pointers.
            let _ = unsafe { PostThreadMessageW(self.host, WAKE, WPARAM(0), LPARAM(0)) };
        }
    }
}

#[implement(IMMNotificationClient)]
struct DeviceEvents(Arc<Wake>);
impl IMMNotificationClient_Impl for DeviceEvents_Impl {
    // Any device arriving, leaving or changing state may change or restore the default endpoint.
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        self.0.raise(REBIND);
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        self.0.raise(REBIND);
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        self.0.raise(REBIND);
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> Result<()> {
        // Only the default console render endpoint is bound; other flows and roles change nothing.
        if flow == eRender && role == eConsole {
            self.0.raise(REBIND);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeEvents(Arc<Wake>);
impl IAudioEndpointVolumeCallback_Impl for VolumeEvents_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        // SAFETY: Core Audio's notification data is valid for this call; a null one still wakes.
        let echo = unsafe { data.as_ref() }.is_some_and(|data| data.guidEventContext == ECHO);
        if !echo {
            self.0.raise(VOLUME);
        }
        Ok(())
    }
}

/// The device enumerator and its registered callback; dropping it unregisters, then releases.
struct Devices {
    enumerator: IMMDeviceEnumerator,
    events: IMMNotificationClient,
}
impl Drop for Devices {
    fn drop(&mut self) {
        // SAFETY: host thread, outside any callback; `events` is the client registered on it.
        if let Err(error) = unsafe {
            self.enumerator
                .UnregisterEndpointNotificationCallback(&self.events)
        } {
            eprintln!("Audio device event cleanup: {error}");
        }
    }
}

/// The bound endpoint's volume and its registered callback; dropping it unregisters, then releases
/// both and the endpoint's meter.
struct Endpoint {
    volume: IAudioEndpointVolume,
    events: IAudioEndpointVolumeCallback,
    /// `None` when the endpoint has no meter, or after a failed read until the next rebind.
    meter: Option<IAudioMeterInformation>,
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        // SAFETY: host thread, outside any callback; `events` is the callback registered on it.
        if let Err(error) = unsafe { self.volume.UnregisterControlChangeNotify(&self.events) } {
            eprintln!("Audio volume event cleanup: {error}");
        }
    }
}

fn bind(devices: &IMMDeviceEnumerator, wake: &Arc<Wake>) -> Result<Endpoint> {
    let events: IAudioEndpointVolumeCallback = VolumeEvents(Arc::clone(wake)).into();
    // SAFETY: host-thread calls on live interfaces. The device is released here, on the host, and
    // `events` stays alive in `Endpoint` until it is unregistered.
    unsafe {
        let device = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
        volume.RegisterControlChangeNotify(&events)?;
        // The volume row works without a meter; the visualizer then stays flat.
        let meter = device
            .Activate(CLSCTX_ALL, None)
            .inspect_err(|error| eprintln!("Audio: no peak meter: {error}"))
            .ok();
        Ok(Endpoint {
            volume,
            events,
            meter,
        })
    }
}

fn read(volume: &IAudioEndpointVolume) -> Result<Level> {
    // SAFETY: host-thread reads on the bound endpoint.
    unsafe {
        Ok(Level {
            percent: percent(volume.GetMasterVolumeLevelScalar()?),
            muted: volume.GetMute()?.as_bool(),
        })
    }
}

/// Host-thread owner of the Core Audio objects. Must be dropped on the host thread before its COM
/// apartment guard.
pub struct Audio {
    // Teardown is field order: the volume callback is unregistered before the device callback.
    endpoint: Option<Endpoint>,
    devices: Option<Devices>,
    started: bool,
    /// `None` without a default render endpoint, or before the first bind.
    level: Option<Level>,
    /// The last `BARS` peak samples, oldest first; zero while the visualizer is stopped.
    peaks: [f32; BARS],
    wake: Arc<Wake>,
}
impl Audio {
    /// Idle until `start`; the calling thread is the host thread that `refresh` runs on.
    pub fn create() -> Self {
        Self {
            endpoint: None,
            devices: None,
            started: false,
            level: None,
            peaks: [0.0; BARS],
            wake: Arc::new(Wake {
                dirty: AtomicU32::new(0),
                // SAFETY: reads this thread's ID; no pointers.
                host: unsafe { GetCurrentThreadId() },
            }),
        }
    }
    /// Subscribes to device changes once; the next `refresh` binds the default endpoint. A failure
    /// leaves the row unavailable and is not retried.
    pub fn start(&mut self) {
        if std::mem::replace(&mut self.started, true) {
            return;
        }
        let events: IMMNotificationClient = DeviceEvents(Arc::clone(&self.wake)).into();
        // SAFETY: host STA thread; `events` stays alive in `Devices` until it is unregistered.
        let devices = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.and_then(
            |enumerator: IMMDeviceEnumerator| {
                // SAFETY: as above.
                unsafe { enumerator.RegisterEndpointNotificationCallback(&events)? };
                Ok(Devices { enumerator, events })
            },
        );
        match devices {
            Ok(devices) => {
                self.devices = Some(devices);
                self.wake.dirty.fetch_or(REBIND, Ordering::Relaxed);
            }
            Err(error) => eprintln!("Audio: device events: {error}"),
        }
    }
    pub fn level(&self) -> Option<Level> {
        self.level
    }
    /// Whether the bound endpoint has a meter for `sample` to read.
    pub fn metered(&self) -> bool {
        self.endpoint
            .as_ref()
            .is_some_and(|endpoint| endpoint.meter.is_some())
    }
    pub fn peaks(&self) -> [f32; BARS] {
        self.peaks
    }
    /// Reads one endpoint-wide peak (all channels and apps, before volume) into the newest bar. A
    /// failed read drops the meter until the next rebind, so a lost device is not polled.
    pub fn sample(&mut self) {
        let Some(endpoint) = &mut self.endpoint else {
            return;
        };
        let Some(meter) = &endpoint.meter else {
            return;
        };
        // SAFETY: host-thread read on the bound endpoint's meter.
        let peak = unsafe { meter.GetPeakValue() }.unwrap_or_else(|error| {
            eprintln!("Audio: peak: {error}");
            endpoint.meter = None;
            0.0
        });
        push(&mut self.peaks, peak);
    }
    /// Flattens the bars, so a restarted visualizer never shows old samples.
    pub fn stop(&mut self) {
        self.peaks = [0.0; BARS];
    }
    /// Applies pending notifications; returns whether the shown level changed. Without any it
    /// costs one atomic swap.
    pub fn refresh(&mut self) -> bool {
        let dirty = self.wake.dirty.swap(0, Ordering::Relaxed);
        if dirty == 0 {
            return false;
        }
        let before = self.level;
        if dirty & REBIND != 0 {
            // The old callback is unregistered before the new endpoint is bound.
            self.endpoint = None;
            self.endpoint = self.devices.as_ref().and_then(|devices| {
                bind(&devices.enumerator, &self.wake)
                    .inspect_err(|error| eprintln!("Audio: no render endpoint: {error}"))
                    .ok()
            });
            self.stop();
            eprintln!("Audio: rebound, endpoint={}", self.endpoint.is_some());
        }
        self.level = self.endpoint.as_ref().and_then(|endpoint| {
            read(&endpoint.volume)
                .inspect_err(|error| eprintln!("Audio: level: {error}"))
                .ok()
        });
        before != self.level
    }
    /// Sets the master volume, 0 to 1, as ROQA's own change, unmuting a muted endpoint; the next
    /// `refresh` reads the result. Like `toggle_mute`, it acts only while the row shows a level,
    /// never while it shows none.
    pub fn set(&self, scalar: f32) {
        if let (Some(endpoint), Some(level)) = (&self.endpoint, self.level) {
            // SAFETY: host-thread calls on the bound endpoint; `ECHO` outlives them.
            let set = unsafe {
                endpoint
                    .volume
                    .SetMasterVolumeLevelScalar(scalar, &ECHO)
                    .and_then(|()| {
                        if unmutes(level) {
                            endpoint.volume.SetMute(false, &ECHO)
                        } else {
                            Ok(())
                        }
                    })
            };
            self.changed(set);
        }
    }
    pub fn toggle_mute(&self) {
        if let (Some(endpoint), Some(level)) = (&self.endpoint, self.level) {
            // SAFETY: as in `set`.
            let set = unsafe { endpoint.volume.SetMute(!level.muted, &ECHO) };
            self.changed(set);
        }
    }
    /// Queues the re-read that the suppressed echo would have asked for.
    fn changed(&self, set: Result<()>) {
        if let Err(error) = set {
            eprintln!("Audio: set: {error}");
        }
        self.wake.dirty.fetch_or(VOLUME, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::Audio::{
        DEVICE_STATE_UNPLUGGED, eCapture, eCommunications, eMultimedia,
    };

    fn wake() -> Arc<Wake> {
        // Thread 0 has no queue, so the posts below fail harmlessly; only the bits matter.
        Arc::new(Wake {
            dirty: AtomicU32::new(0),
            host: 0,
        })
    }
    fn take(wake: &Wake) -> u32 {
        wake.dirty.swap(0, Ordering::Relaxed)
    }

    #[test]
    fn only_the_default_console_render_endpoint_and_device_changes_rebind() {
        let wake = wake();
        let client: IMMNotificationClient = DeviceEvents(Arc::clone(&wake)).into();
        let id = PCWSTR::null();
        // SAFETY: calls into our own in-process callback object, which only touches `wake`.
        unsafe {
            client
                .OnDefaultDeviceChanged(eRender, eConsole, id)
                .unwrap();
            assert_eq!(take(&wake), REBIND);
            for (flow, role) in [
                (eRender, eMultimedia),
                (eRender, eCommunications),
                (eCapture, eConsole),
            ] {
                client.OnDefaultDeviceChanged(flow, role, id).unwrap();
                assert_eq!(take(&wake), 0);
            }
            client.OnDeviceRemoved(id).unwrap();
            assert_eq!(take(&wake), REBIND);
            client.OnDeviceAdded(id).unwrap();
            assert_eq!(take(&wake), REBIND);
            client
                .OnDeviceStateChanged(id, DEVICE_STATE_UNPLUGGED)
                .unwrap();
            assert_eq!(take(&wake), REBIND);
            client
                .OnPropertyValueChanged(id, PROPERTYKEY::default())
                .unwrap();
            assert_eq!(take(&wake), 0);
        }
    }

    #[test]
    fn our_own_changes_echo_without_a_wake_and_others_wake() {
        let wake = wake();
        let callback: IAudioEndpointVolumeCallback = VolumeEvents(Arc::clone(&wake)).into();
        let mut data = AUDIO_VOLUME_NOTIFICATION_DATA {
            guidEventContext: ECHO,
            ..Default::default()
        };
        // SAFETY: our own callback object, given valid or null notification data.
        unsafe {
            callback.OnNotify(&mut data).unwrap();
            assert_eq!(take(&wake), 0);
            data.guidEventContext = GUID::zeroed();
            callback.OnNotify(&mut data).unwrap();
            assert_eq!(take(&wake), VOLUME);
            callback.OnNotify(std::ptr::null_mut()).unwrap();
            assert_eq!(take(&wake), VOLUME);
        }
    }

    #[test]
    fn only_a_whole_percent_or_mute_change_repaints() {
        assert_eq!(
            (percent(0.0), percent(1.0), percent(-1.0), percent(2.0)),
            (0, 100, 0, 100)
        );
        assert_eq!(percent(f32::NAN), 0);
        let shown = |scalar, muted| {
            Some(Level {
                percent: percent(scalar),
                muted,
            })
        };
        assert_eq!(shown(0.401, false), shown(0.399, false));
        assert_ne!(shown(0.40, false), shown(0.41, false));
        assert_ne!(shown(0.40, false), shown(0.40, true));
        // Without an endpoint, a wake re-reads nothing and repaints nothing.
        let mut audio = Audio::create();
        assert!(!audio.refresh());
        audio.wake.raise(VOLUME);
        assert!(!audio.refresh());
        audio.level = shown(0.4, false);
        audio.wake.raise(REBIND);
        assert!(audio.refresh()); // A lost endpoint hides the level.
        assert_eq!(audio.level(), None);
    }

    #[test]
    fn setting_the_level_unmutes_only_a_muted_endpoint() {
        let level = |muted| Level { percent: 40, muted };
        assert!(unmutes(level(true)));
        assert!(!unmutes(level(false)));
    }

    #[test]
    fn peaks_shift_the_oldest_out_clamp_and_reset_on_rebind() {
        let mut peaks = [0.0; BARS];
        for peak in [0.1, 0.2, 0.3, 0.4, 0.5, 0.6] {
            push(&mut peaks, peak);
        }
        assert_eq!(peaks, [0.2, 0.3, 0.4, 0.5, 0.6]);
        for peak in [-1.0, 2.0, f32::NAN] {
            push(&mut peaks, peak);
        }
        assert_eq!(peaks, [0.5, 0.6, 0.0, 1.0, 0.0]);
        // Without an endpoint there is no meter: sampling makes no call and shifts nothing.
        let mut audio = Audio::create();
        audio.peaks = peaks;
        assert!(!audio.metered());
        audio.sample();
        assert_eq!(audio.peaks(), peaks);
        audio.wake.raise(REBIND);
        audio.refresh();
        assert_eq!(audio.peaks(), [0.0; BARS]);
    }
}
