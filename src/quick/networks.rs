use super::wait;
use std::sync::{
    Mutex, PoisonError,
    atomic::{AtomicBool, Ordering},
};
use windows::{
    Devices::Enumeration::{DeviceInformation, DeviceInformationKind},
    Foundation::IPropertyValue,
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WIN32_ERROR},
        NetworkManagement::WiFi::*,
        System::{
            Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
            Threading::{CreateEventW, GetCurrentThreadId, SetEvent, WaitForSingleObject},
        },
    },
    core::{GUID, HSTRING, Interface, PCWSTR, Result},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    WiFi,
    Bluetooth,
}

#[derive(Clone)]
pub struct Row {
    pub name: String,
    pub detail: String,
    pub connected: bool,
    pub(crate) profile: Option<(GUID, String)>,
}

static BUSY: AtomicBool = AtomicBool::new(false);
/// The page, its rows, and whether a connect attempt failed.
static ANSWER: Mutex<Option<(Page, Vec<Row>, bool)>> = Mutex::new(None);
const READY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 120;

struct Client(HANDLE);
impl Client {
    fn open() -> Result<Self> {
        let mut handle = HANDLE::default();
        let mut version = 0;
        // SAFETY: the outputs are writable and the reserved pointer is null.
        unsafe {
            WIN32_ERROR(WlanOpenHandle(2, None, &mut version, &mut handle)).ok()?;
        }
        Ok(Self(handle))
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        // SAFETY: this guard owns the WLAN client handle.
        unsafe {
            WlanCloseHandle(self.0, None);
        }
    }
}

struct Buffer<T>(*mut T);
impl<T> Drop for Buffer<T> {
    fn drop(&mut self) {
        // SAFETY: this guard owns the buffer returned by WLAN.
        unsafe {
            WlanFreeMemory(self.0.cast());
        }
    }
}

fn saved_networks() -> Result<Vec<Row>> {
    let client = Client::open()?;
    // SAFETY: successful WLAN calls own their buffers until the guards drop; list lengths are supplied with each list.
    unsafe {
        let mut interfaces = std::ptr::null_mut();
        WIN32_ERROR(WlanEnumInterfaces(client.0, None, &mut interfaces)).ok()?;
        let interfaces = Buffer(interfaces);
        let interfaces = std::slice::from_raw_parts(
            (*interfaces.0).InterfaceInfo.as_ptr(),
            (*interfaces.0).dwNumberOfItems as usize,
        );
        let mut rows = Vec::new();
        for interface in interfaces {
            let mut networks = std::ptr::null_mut();
            WIN32_ERROR(WlanGetAvailableNetworkList(
                client.0,
                &interface.InterfaceGuid,
                0,
                None,
                &mut networks,
            ))
            .ok()?;
            let networks = Buffer(networks);
            let networks = std::slice::from_raw_parts(
                (*networks.0).Network.as_ptr(),
                (*networks.0).dwNumberOfItems as usize,
            );
            for network in networks {
                let profile = String::from_utf16_lossy(
                    &network.strProfileName[..network
                        .strProfileName
                        .iter()
                        .position(|&unit| unit == 0)
                        .unwrap_or(network.strProfileName.len())],
                );
                if profile.is_empty() || !network.bNetworkConnectable.as_bool() {
                    continue;
                }
                let ssid = &network.dot11Ssid;
                let name = String::from_utf8_lossy(
                    &ssid.ucSSID[..(ssid.uSSIDLength as usize).min(ssid.ucSSID.len())],
                )
                .into_owned();
                let connected = network.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED != 0;
                if rows.iter().any(|row: &Row| {
                    row.profile.as_ref().is_some_and(|(guid, saved)| {
                        *guid == interface.InterfaceGuid && *saved == profile
                    })
                }) {
                    continue;
                }
                rows.push(Row {
                    name: if name.is_empty() {
                        profile.clone()
                    } else {
                        name
                    },
                    detail: if connected { "Connected" } else { "Saved" }.into(),
                    connected,
                    profile: Some((interface.InterfaceGuid, profile)),
                });
            }
        }
        rows.sort_by(|left, right| {
            right
                .connected
                .cmp(&left.connected)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(rows)
    }
}

struct ConnectionWait {
    guid: GUID,
    profile: HSTRING,
    event: HANDLE,
    connected: AtomicBool,
}

impl Drop for ConnectionWait {
    fn drop(&mut self) {
        // SAFETY: the client is closed before this guard drops, joining its callbacks.
        unsafe {
            let _ = CloseHandle(self.event);
        }
    }
}

const COMPLETE: u32 = wlan_notification_acm_connection_complete.0 as u32;
const ATTEMPT_FAIL: u32 = wlan_notification_acm_connection_attempt_fail.0 as u32;

/// Only a completed connection with a success reason means Windows joined the network.
fn succeeded(code: u32, reason: u32) -> bool {
    code == COMPLETE && reason == WLAN_REASON_CODE_SUCCESS
}

unsafe extern "system" fn connection_changed(
    data: *mut L2_NOTIFICATION_DATA,
    context: *mut core::ffi::c_void,
) {
    // SAFETY: WLAN owns `data` during the callback; `context` is kept alive until WlanCloseHandle returns.
    let (Some(data), Some(wait)) = (unsafe { data.as_ref() }, unsafe {
        context.cast::<ConnectionWait>().as_ref()
    }) else {
        return;
    };
    if data.NotificationSource != WLAN_NOTIFICATION_SOURCE_ACM
        || data.InterfaceGuid != wait.guid
        || ![COMPLETE, ATTEMPT_FAIL].contains(&data.NotificationCode)
        || (data.dwDataSize as usize) < std::mem::size_of::<WLAN_CONNECTION_NOTIFICATION_DATA>()
    {
        return;
    }
    // SAFETY: this notification kind carries WLAN_CONNECTION_NOTIFICATION_DATA, sized above.
    let Some(connection) = (unsafe {
        data.pData
            .cast::<WLAN_CONNECTION_NOTIFICATION_DATA>()
            .as_ref()
    }) else {
        return;
    };
    let length = connection
        .strProfileName
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(connection.strProfileName.len());
    if connection.strProfileName[..length] == *wait.profile {
        // A later failure notice never undoes a success already seen.
        wait.connected.fetch_or(
            succeeded(data.NotificationCode, connection.wlanReasonCode),
            Ordering::AcqRel,
        );
        // SAFETY: the event remains live until the client has finished all callbacks.
        unsafe {
            let _ = SetEvent(wait.event);
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "bounded wait on the network worker, never the host"
)]
/// `Ok(false)` when Windows refused the profile or did not answer in time.
fn connect(guid: GUID, profile: String) -> Result<bool> {
    // SAFETY: default security attributes and no name for this private event.
    let event = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }?;
    let wait = ConnectionWait {
        guid,
        profile: HSTRING::from(profile),
        event,
        connected: AtomicBool::new(false),
    };
    let client = Client::open()?;
    let parameters = WLAN_CONNECTION_PARAMETERS {
        wlanConnectionMode: wlan_connection_mode_profile,
        strProfile: PCWSTR(wait.profile.as_ptr()),
        dot11BssType: dot11_BSS_type_any,
        ..Default::default()
    };
    // SAFETY: the client is live and the terminated profile and parameters outlive the call.
    unsafe {
        WIN32_ERROR(WlanRegisterNotification(
            client.0,
            WLAN_NOTIFICATION_SOURCE_ACM,
            true,
            Some(connection_changed),
            Some(std::ptr::from_ref(&wait).cast()),
            None,
            None,
        ))
        .ok()?;
        WIN32_ERROR(WlanConnect(client.0, &guid, &parameters, None)).ok()?;
        if WaitForSingleObject(wait.event, 8_000) != WAIT_OBJECT_0 {
            return Ok(false);
        }
    }
    Ok(wait.connected.load(Ordering::Acquire))
}

fn paired_devices() -> Result<Vec<Row>> {
    const CONNECTED: &str = "System.Devices.Aep.IsConnected";
    const BATTERY: &str = "System.Devices.BatteryLife";
    const CONTAINER: &str = "System.Devices.ContainerId";
    let filter = HSTRING::from(
        r#"(System.Devices.Aep.ProtocolId:="{e0cbf06c-cd8b-4647-bb8a-263b43f0f974}" OR System.Devices.Aep.ProtocolId:="{bb7bb05e-5972-42b5-94fc-76eaa7084d49}") AND System.Devices.Aep.IsPaired:=System.StructuredQueryType.Boolean#True"#,
    );
    let properties = windows_collections::IIterable::from(
        [CONNECTED, BATTERY, CONTAINER].map(HSTRING::from).to_vec(),
    );
    let devices = wait(
        &DeviceInformation::FindAllAsyncWithKindAqsFilterAndAdditionalProperties(
            &filter,
            &properties,
            DeviceInformationKind::AssociationEndpoint,
        )?,
    )?;
    let mut seen = std::collections::BTreeSet::new();
    let mut rows = Vec::new();
    for device in devices {
        let name = device.Name()?.to_string();
        if name.trim().is_empty() {
            continue;
        }
        let props = device.Properties()?;
        let property = |key: &str| {
            props
                .Lookup(&HSTRING::from(key))
                .ok()?
                .cast::<IPropertyValue>()
                .ok()
        };
        let connected = property(CONNECTED)
            .and_then(|value| value.GetBoolean().ok())
            .unwrap_or(false);
        let container = property(CONTAINER).and_then(|value| value.GetGuid().ok());
        let battery = property(BATTERY)
            .and_then(|value| value.GetUInt8().ok())
            .filter(|&value| value <= 100);
        let identity = container.map_or_else(|| name.to_lowercase(), |id| format!("{id:?}"));
        if !seen.insert(identity) {
            if connected && let Some(row) = rows.iter_mut().find(|row: &&mut Row| row.name == name)
            {
                row.connected = true;
                row.detail = battery.map_or_else(
                    || "Connected".into(),
                    |percent| format!("Connected · {percent}%"),
                );
            }
            continue;
        }
        let detail = if connected {
            battery.map_or_else(
                || "Connected".into(),
                |percent| format!("Connected · {percent}%"),
            )
        } else {
            "Paired".into()
        };
        rows.push(Row {
            name,
            detail,
            connected,
            profile: None,
        });
    }
    rows.sort_by(|left, right| {
        right
            .connected
            .cmp(&left.connected)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(rows)
}

fn unavailable() -> Vec<Row> {
    vec![Row {
        name: "Windows access unavailable".into(),
        detail: String::new(),
        connected: false,
        profile: None,
    }]
}

/// Starts a list (or connect) job; false only while an earlier job or its unread answer holds the slot.
pub fn request(page: Page, row: Option<&Row>) -> bool {
    let profile = row.and_then(|row| row.profile.clone());
    if BUSY.swap(true, Ordering::AcqRel) {
        return false;
    }
    let connecting = profile.is_some();
    // SAFETY: reads this thread ID without pointers.
    let host = unsafe { GetCurrentThreadId() };
    let sent = std::thread::Builder::new()
        .name("roqa-network-list".into())
        .spawn(move || {
            // SAFETY: initializes COM on this worker only.
            let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            let mut failed = false;
            let result = if let Some((guid, profile)) = profile {
                failed = !connect(guid, profile).unwrap_or_else(|error| {
                    eprintln!("Wi-Fi connect: {error}");
                    false
                });
                saved_networks()
            } else if page == Page::WiFi {
                saved_networks()
            } else {
                paired_devices()
            };
            if com {
                // SAFETY: balances initialization on the same worker.
                unsafe { CoUninitialize() };
            }
            let rows = result.unwrap_or_else(|error| {
                eprintln!("Command center: {error}");
                unavailable()
            });
            *ANSWER.lock().unwrap_or_else(PoisonError::into_inner) = Some((page, rows, failed));
            let _ = crate::native_window::post_wake(host, READY);
        })
        .is_ok();
    if !sent {
        // The host reads the slot on every pass, so a failed start still leaves the page an answer.
        *ANSWER.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((page, unavailable(), connecting));
    }
    true
}

/// Takes the finished answer and frees the slot for the next job, so no job overwrites an unread answer.
pub fn answer() -> Option<(Page, Vec<Row>, bool)> {
    let answer = ANSWER.lock().unwrap_or_else(PoisonError::into_inner).take();
    if answer.is_some() {
        BUSY.store(false, Ordering::Release);
    }
    answer
}

impl Row {
    pub fn can_connect(&self) -> bool {
        self.profile.is_some() && !self.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_successful_completion_connects() {
        assert!(succeeded(COMPLETE, WLAN_REASON_CODE_SUCCESS));
        assert!(!succeeded(COMPLETE, WLAN_REASON_CODE_SUCCESS + 1));
        assert!(!succeeded(ATTEMPT_FAIL, WLAN_REASON_CODE_SUCCESS));
    }
}
