//! The default render endpoint's master volume and mute, for the media panel's volume row, and its
//! peak meter, for the visualizer.
//!
//! Core Audio starts the first time a media notch is shown and stays bound until shutdown. Its
//! callbacks may run on any Core Audio thread: they only set a dirty bit and post one scalar wake,
//! so they never block, panic, register or unregister. Every bind, read and write happens on the
//! host (STA) thread.
use std::sync::{
    Arc,
    atomic::{AtomicU32, AtomicU64, Ordering},
};
use windows::{
    Win32::{
        Foundation::{LPARAM, PROPERTYKEY, WPARAM},
        Media::Audio::{
            AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole,
            Endpoints::{
                IAudioEndpointVolume, IAudioEndpointVolumeCallback,
                IAudioEndpointVolumeCallback_Impl, IAudioMeterInformation,
            },
            IMMDevice, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
            MMDeviceEnumerator, eConsole, eRender,
        },
        System::{
            Com::{
                CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ,
                StructuredStorage::PropVariantToStringAlloc,
            },
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

/// The most render endpoints the slider's device list shows.
pub const MAX_DEVICES: usize = 8;
/// `PKEY_Device_FriendlyName`, as a constant of our own rather than the whole function-discovery feature.
const FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};

/// An active render endpoint: its ID, its name, and whether it is the default now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub current: bool,
}

/// `(id, name)` pairs as enumerated, bounded, with the one equal to `default` marked current.
fn mark_current(found: Vec<(String, String)>, default: Option<&str>) -> Vec<Device> {
    found
        .into_iter()
        .take(MAX_DEVICES)
        .map(|(id, name)| Device {
            current: default == Some(id.as_str()),
            id,
            name,
        })
        .collect()
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

/// A nonzero FNV-1a hash of an endpoint ID, so callbacks can compare IDs without locking.
fn id_hash(id: &[u16]) -> u64 {
    let hash = id.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &unit| {
        (hash ^ u64::from(unit)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    hash.max(1)
}

/// Whether a device change can affect the bound endpoint: any change while none is known (`bound`
/// 0), or one whose ID is unknown or the bound one's. Other devices' changes bind nothing new.
fn affects(bound: u64, changed: Option<u64>) -> bool {
    bound == 0 || changed.is_none_or(|changed| changed == bound)
}

/// The only state callbacks touch.
struct Wake {
    dirty: AtomicU32,
    /// `id_hash` of the bound endpoint, or 0 while none is.
    bound: AtomicU64,
    host: u32,
}
impl Wake {
    /// `raise(REBIND)` for a change to endpoint `id`, unless it cannot concern the bound one.
    fn rebind_for(&self, id: &PCWSTR) {
        // SAFETY: Core Audio passes a NUL-terminated endpoint ID valid for this call; null is skipped.
        let changed = (!id.is_null()).then(|| id_hash(unsafe { id.as_wide() }));
        if affects(self.bound.load(Ordering::Relaxed), changed) {
            self.raise(REBIND);
        }
    }
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
    // The bound endpoint changing state, or any device while none is bound, may need a new bind;
    // other devices (capture, Bluetooth flurries) change nothing. A default switch is the next event.
    fn OnDeviceStateChanged(&self, id: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        self.0.rebind_for(id);
        Ok(())
    }
    fn OnDeviceAdded(&self, id: &PCWSTR) -> Result<()> {
        self.0.rebind_for(id);
        Ok(())
    }
    fn OnDeviceRemoved(&self, id: &PCWSTR) -> Result<()> {
        self.0.rebind_for(id);
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
        let id = device.GetId()?;
        wake.bound.store(id_hash(id.as_wide()), Ordering::Relaxed);
        CoTaskMemFree(Some(id.0.cast_const().cast()));
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

/// # Safety
/// Host thread, on a live device.
unsafe fn endpoint_id(device: &IMMDevice) -> Result<String> {
    // SAFETY: per the contract; the ID is copied and freed once.
    unsafe {
        let id = device.GetId()?;
        let text = id.to_string();
        CoTaskMemFree(Some(id.0.cast_const().cast()));
        text.map_err(|_| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))
    }
}

/// The endpoint's friendly name, such as "Speakers (Realtek Audio)"; `None` when unreadable.
///
/// # Safety
/// Host thread, on a live device.
unsafe fn endpoint_name(device: &IMMDevice) -> Option<String> {
    // SAFETY: per the contract; the property value owns its string and the copy is freed once.
    unsafe {
        let value = device
            .OpenPropertyStore(STGM_READ)
            .ok()?
            .GetValue(&FRIENDLY_NAME)
            .ok()?;
        let text = PropVariantToStringAlloc(&value).ok()?;
        let name = text.to_string().ok();
        CoTaskMemFree(Some(text.0.cast_const().cast()));
        name.filter(|name| !name.is_empty())
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
                bound: AtomicU64::new(0),
                // SAFETY: reads this thread's ID; no pointers.
                host: unsafe { GetCurrentThreadId() },
            }),
        }
    }
    /// Subscribes to device changes once; the next `refresh` binds the default endpoint. A failure
    /// leaves the row unavailable until the next `reconnect` allows another try.
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
    /// Core Audio has been started (a level may still be missing without a device).
    pub fn active(&self) -> bool {
        self.started
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
    /// Work is pending that no wake may have announced (see `Media::pending`).
    pub fn pending(&self) -> bool {
        self.wake.dirty.load(Ordering::Relaxed) != 0
    }
    /// After resume or an Explorer restart, binds the default endpoint and its meter again, as a
    /// device may have changed or been invalidated while events were missed. After a failed `start`
    /// (no enumerator, say while AudioSrv is down at logon), the next `start` tries again.
    pub fn reconnect(&mut self) {
        if self.devices.is_some() {
            self.wake.dirty.fetch_or(REBIND, Ordering::Relaxed);
        } else {
            self.started = false;
        }
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
            self.wake.bound.store(0, Ordering::Relaxed);
            self.endpoint = self.devices.as_ref().and_then(|devices| {
                bind(&devices.enumerator, &self.wake)
                    .inspect_err(|error| eprintln!("Audio: no render endpoint: {error}"))
                    .ok()
            });
            self.stop();
            std::eprintln!("Audio: rebound, endpoint={}", self.endpoint.is_some());
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
    /// The active render endpoints and the default one (host thread). Empty before `start`, or when the
    /// enumerator fails, in which case the caller falls back to the Sound settings page.
    pub fn devices(&self) -> Vec<Device> {
        let Some(devices) = &self.devices else {
            return Vec::new();
        };
        // SAFETY: host-thread calls on the live enumerator; every string is freed once.
        unsafe {
            let default = devices
                .enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .and_then(|device| endpoint_id(&device))
                .ok();
            let found = devices
                .enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .and_then(|list| {
                    (0..list.GetCount()?.min(MAX_DEVICES as u32))
                        .map(|index| {
                            let device = list.Item(index)?;
                            let id = endpoint_id(&device)?;
                            let name = endpoint_name(&device).unwrap_or_else(|| id.clone());
                            Ok((id, name))
                        })
                        .collect()
                });
            found
                .map(|found| mark_current(found, default.as_deref()))
                .inspect_err(|error| eprintln!("Audio: device list: {error}"))
                .unwrap_or_default()
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
    use windows::{
        Win32::Media::Audio::{DEVICE_STATE_UNPLUGGED, eCapture, eCommunications, eMultimedia},
        core::w,
    };

    fn wake() -> Arc<Wake> {
        // Thread 0 has no queue, so the posts below fail harmlessly; only the bits matter.
        Arc::new(Wake {
            dirty: AtomicU32::new(0),
            bound: AtomicU64::new(0),
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
    fn other_devices_changes_do_not_rebind_and_a_failed_start_retries_on_reconnect() {
        let id = |text: &str| id_hash(&text.encode_utf16().collect::<Vec<_>>());
        assert_ne!(id("{a}"), id("{b}"));
        assert_ne!(id(""), 0);
        assert!(affects(0, Some(id("{a}"))));
        assert!(affects(id("{a}"), None));
        assert!(affects(id("{a}"), Some(id("{a}"))));
        assert!(!affects(id("{a}"), Some(id("{b}"))));
        let wake = wake();
        wake.bound.store(id("{bound}"), Ordering::Relaxed);
        let client: IMMNotificationClient = DeviceEvents(Arc::clone(&wake)).into();
        // SAFETY: calls into our own in-process callback object, which only touches `wake`.
        unsafe {
            client
                .OnDeviceStateChanged(w!("{other}"), DEVICE_STATE_UNPLUGGED)
                .unwrap();
            client.OnDeviceAdded(w!("{other}")).unwrap();
            client.OnDeviceRemoved(w!("{other}")).unwrap();
            assert_eq!(take(&wake), 0);
            client
                .OnDeviceStateChanged(w!("{bound}"), DEVICE_STATE_UNPLUGGED)
                .unwrap();
            assert_eq!(take(&wake), REBIND);
            client.OnDeviceRemoved(w!("{bound}")).unwrap();
            assert_eq!(take(&wake), REBIND);
        }
        // Nothing bound: any device may be the one to bind.
        wake.bound.store(0, Ordering::Relaxed);
        // SAFETY: as above.
        unsafe { client.OnDeviceAdded(w!("{other}")).unwrap() };
        assert_eq!(take(&wake), REBIND);
        // A start that never got an enumerator is tried again, with no wake queued.
        let mut audio = Audio::create();
        audio.started = true;
        audio.reconnect();
        assert!(!audio.started && !audio.pending());
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
    fn the_device_list_marks_the_default_and_stays_bounded() {
        let found = |n: usize| -> Vec<(String, String)> {
            (0..n)
                .map(|i| (format!("{{id{i}}}"), format!("Device {i}")))
                .collect()
        };
        let list = mark_current(found(3), Some("{id1}"));
        assert_eq!(
            list.iter().map(|d| d.current).collect::<Vec<_>>(),
            [false, true, false]
        );
        assert_eq!(list[1].name, "Device 1");
        // An unknown or missing default marks nothing; the list never exceeds the bound.
        assert!(
            mark_current(found(3), Some("{gone}"))
                .iter()
                .all(|d| !d.current)
        );
        assert!(mark_current(found(3), None).iter().all(|d| !d.current));
        assert_eq!(mark_current(found(20), None).len(), MAX_DEVICES);
        assert!(mark_current(Vec::new(), None).is_empty());
        // Without a started enumerator there is nothing to list.
        assert!(Audio::create().devices().is_empty());
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
