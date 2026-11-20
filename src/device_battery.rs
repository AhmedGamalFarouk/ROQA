//! Wireless-device battery reports supplied by Windows, never vendor protocols.
//! Watcher callbacks run off the host; the host reads only a published text snapshot.
use std::{
    collections::BTreeMap,
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Devices::Enumeration::{DeviceInformation, DeviceInformationKind, DeviceWatcher},
    Foundation::{IPropertyValue, TypedEventHandler},
    core::{HSTRING, Interface, Result},
};

const BATTERY: &str = "System.Devices.BatteryLife";
const CONNECTED: &str = "System.Devices.Connected";
const PAIRED: &str = "System.Devices.Paired";
const LOCAL: &str = "System.Devices.LocalMachine";
const CONTAINER: &str = "System.Devices.ContainerId";
const NAME: &str = "System.ItemNameDisplay";
const READY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 89;
static ROWS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static CHANGED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);
static WATCHERS: Mutex<Vec<DeviceWatcher>> = Mutex::new(Vec::new());
static ERROR: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, Default)]
struct Report {
    name: String,
    container: String,
    connected: bool,
    paired: bool,
    local: bool,
    percent: Option<u8>,
    battery_reported: bool,
}

fn identity(id: &str) -> String {
    id.trim_matches(['{', '}']).to_ascii_lowercase()
}

/// A container is one physical device; child interfaces may carry its battery reading.
fn rows(containers: &BTreeMap<String, Report>, devices: &BTreeMap<String, Report>) -> Vec<String> {
    let mut result = Vec::new();
    for (id, container) in containers {
        if !container.connected || container.local {
            continue;
        }
        let mut values = devices
            .values()
            .filter(|device| device.container == *id)
            .filter_map(|device| device.percent);
        let first = values.next();
        // Conflicting child readings cannot be represented as a single trustworthy percentage.
        let percent = container
            .percent
            .or(first.filter(|value| values.all(|other| other == *value)));
        let battery_reported = container.battery_reported
            || devices
                .values()
                .any(|device| device.container == *id && device.battery_reported);
        if !container.paired && !battery_reported && container.percent.is_none() && first.is_none()
        {
            continue;
        }
        let name = if container.name.trim().is_empty() {
            "Wireless device"
        } else {
            &container.name
        };
        result.push(format!(
            "{name}  ·  {}",
            percent.map_or_else(|| "Unavailable".into(), |value| format!("{value}%"))
        ));
    }
    result.sort_by_key(|row| row.to_lowercase());
    result
}

pub fn titles() -> Vec<String> {
    if ERROR.load(Ordering::Acquire) {
        return vec!["Device reports unavailable".into()];
    }
    let rows = ROWS.lock().unwrap_or_else(PoisonError::into_inner).clone();
    if rows.is_empty() {
        vec![
            if STARTED.load(Ordering::Acquire) {
                "No wireless devices reported"
            } else {
                "Reading devices"
            }
            .into(),
        ]
    } else {
        rows
    }
}

pub fn changed() -> bool {
    CHANGED.swap(false, Ordering::AcqRel)
}

fn wake(host: u32) {
    CHANGED.store(true, Ordering::Release);
    let _ = crate::native_window::post_wake(host, READY);
}

fn property(info: &DeviceInformation, key: &str) -> Option<IPropertyValue> {
    info.Properties()
        .ok()?
        .Lookup(&HSTRING::from(key))
        .ok()?
        .cast()
        .ok()
}
fn flag(info: &DeviceInformation, key: &str) -> bool {
    property(info, key)
        .and_then(|value| value.GetBoolean().ok())
        .unwrap_or(false)
}
fn report(info: &DeviceInformation) -> Report {
    let battery = property(info, BATTERY).and_then(|value| value.GetUInt8().ok());
    Report {
        name: info.Name().map(|name| name.to_string()).unwrap_or_default(),
        container: property(info, CONTAINER)
            .and_then(|value| value.GetGuid().ok())
            .map(|id| identity(&format!("{id:?}")))
            .unwrap_or_default(),
        connected: flag(info, CONNECTED),
        paired: flag(info, PAIRED),
        local: flag(info, LOCAL),
        percent: battery.and_then(percentage),
        battery_reported: battery.is_some(),
    }
}

fn percentage(value: u8) -> Option<u8> {
    (value <= 100).then_some(value)
}

#[derive(Default)]
struct Catalog {
    infos: BTreeMap<(bool, String), DeviceInformation>,
    containers: BTreeMap<String, Report>,
    devices: BTreeMap<String, Report>,
    complete: [bool; 2],
}
impl Catalog {
    fn insert(&mut self, container: bool, info: DeviceInformation, fresh: bool) -> Result<()> {
        let id = identity(&info.Id()?.to_string());
        let value = report(&info);
        self.store(container, &id, value, fresh);
        self.infos.insert((container, id), info);
        Ok(())
    }
    fn store(&mut self, container: bool, id: &str, mut value: Report, fresh: bool) {
        let previous = if container {
            self.containers.get(id)
        } else {
            self.devices.get(id)
        };
        value.battery_reported |= previous.is_some_and(|report| report.battery_reported);
        if !fresh {
            value.percent = previous.and_then(|report| report.percent);
        }
        if container {
            if !value.connected || previous.is_some_and(|report| !report.connected) && !fresh {
                value.percent = None;
                self.clear_children(id);
            }
            self.containers.insert(id.into(), value);
        } else {
            if self
                .containers
                .get(&value.container)
                .is_some_and(|parent| !parent.connected)
            {
                value.percent = None;
            }
            self.devices.insert(id.into(), value);
        }
    }
    fn clear_children(&mut self, id: &str) {
        for child in self
            .devices
            .values_mut()
            .filter(|child| child.container == id)
        {
            child.percent = None;
        }
    }
    fn remove(&mut self, container: bool, id: &str) {
        let id = identity(id);
        self.infos.remove(&(container, id.clone()));
        if container {
            self.containers.remove(&id);
            self.clear_children(&id);
        } else {
            self.devices.remove(&id);
        }
    }
    fn publish(&self, host: u32) {
        if !self.complete.into_iter().all(|complete| complete) {
            return;
        }
        let next = rows(&self.containers, &self.devices);
        let mut last = ROWS.lock().unwrap_or_else(PoisonError::into_inner);
        let initial = !STARTED.swap(true, Ordering::AcqRel);
        if !initial && *last == next {
            return;
        }
        *last = next;
        drop(last);
        wake(host);
    }
}

fn watcher(
    container: bool,
    host: u32,
    state: std::sync::Arc<Mutex<Catalog>>,
) -> Result<DeviceWatcher> {
    let properties: Vec<HSTRING> = [BATTERY, CONNECTED, PAIRED, LOCAL, CONTAINER, NAME]
        .map(HSTRING::from)
        .into();
    let kind = if container {
        DeviceInformationKind::DeviceContainer
    } else {
        DeviceInformationKind::Device
    };
    let filter = HSTRING::from(if container {
        ""
    } else {
        "System.Devices.Present:=System.StructuredQueryType.Boolean#True"
    });
    let watcher = DeviceInformation::CreateWatcherWithKindAqsFilterAndAdditionalProperties(
        &filter,
        Some(&properties.into()),
        kind,
    )?;
    let added = state.clone();
    watcher.Added(&TypedEventHandler::new(
        move |_, info: windows::core::Ref<DeviceInformation>| {
            if let Some(info) = info.as_ref() {
                let mut catalog = added.lock().unwrap_or_else(PoisonError::into_inner);
                catalog.insert(container, info.clone(), true)?;
                catalog.publish(host);
            }
            Ok(())
        },
    ))?;
    let updated = state.clone();
    watcher.Updated(&TypedEventHandler::new(
        move |_,
              update: windows::core::Ref<
            windows::Devices::Enumeration::DeviceInformationUpdate,
        >| {
            if let Some(update) = update.as_ref() {
                let mut catalog = updated.lock().unwrap_or_else(PoisonError::into_inner);
                let id = identity(&update.Id()?.to_string());
                if let Some(info) = catalog.infos.get(&(container, id)).cloned() {
                    info.Update(update)?;
                    catalog.insert(
                        container,
                        info,
                        update.Properties()?.HasKey(&HSTRING::from(BATTERY))?,
                    )?;
                    catalog.publish(host);
                }
            }
            Ok(())
        },
    ))?;
    let removed = state.clone();
    watcher.Removed(&TypedEventHandler::new(
        move |_,
              update: windows::core::Ref<
            windows::Devices::Enumeration::DeviceInformationUpdate,
        >| {
            if let Some(update) = update.as_ref() {
                let mut catalog = removed.lock().unwrap_or_else(PoisonError::into_inner);
                catalog.remove(container, &update.Id()?.to_string());
                catalog.publish(host);
            }
            Ok(())
        },
    ))?;
    watcher.EnumerationCompleted(&TypedEventHandler::new(move |_, _| {
        let mut catalog = state.lock().unwrap_or_else(PoisonError::into_inner);
        catalog.complete[usize::from(container)] = true;
        catalog.publish(host);
        Ok(())
    }))?;
    watcher.Stopped(&TypedEventHandler::new(move |_, _| {
        ERROR.store(true, Ordering::Release);
        ROWS.lock().unwrap_or_else(PoisonError::into_inner).clear();
        wake(host);
        Ok(())
    }))?;
    Ok(watcher)
}

/// One bounded setup job; Windows owns event dispatch after it returns. No parked worker or polling.
pub fn start() {
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    // SAFETY: a scalar thread-id query on the host.
    let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let spawned = std::thread::Builder::new()
        .name("roqa-device-battery".into())
        .spawn(move || {
            // SAFETY: a fresh worker; balanced below on this thread.
            let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            let setup = || -> Result<()> {
                if !com {
                    return Err(windows::core::Error::from_hresult(
                        windows::Win32::Foundation::E_FAIL,
                    ));
                }
                let state = std::sync::Arc::new(Mutex::new(Catalog::default()));
                let containers = watcher(true, host, state.clone())?;
                let devices = watcher(false, host, state)?;
                let mut watchers = WATCHERS.lock().unwrap_or_else(PoisonError::into_inner);
                watchers.extend([containers, devices]);
                for watcher in watchers.iter() {
                    watcher.Start()?;
                }
                Ok(())
            };
            if let Err(error) = setup() {
                eprintln!("Device battery: {error}");
                ERROR.store(true, Ordering::Release);
                wake(host);
            }
            if com {
                // SAFETY: balances this worker's successful initialization.
                unsafe { CoUninitialize() };
            }
        });
    if let Err(error) = spawned {
        eprintln!("Device battery worker: {error}");
        ERROR.store(true, Ordering::Release);
        wake(host);
    }
}

#[cfg(test)]
mod tests;
