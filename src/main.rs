#![windows_subsystem = "windows"]

// Keep existing diagnostic stderr output; the fallback records only fixed context, never arguments.
macro_rules! eprintln {
    ($message:literal $(, $argument:expr)* $(,)?) => {{
        std::eprintln!($message $(, $argument)*);
        crate::error_log::record($message);
    }};
}
mod agent;
mod agent_hooks;
mod anchored;
mod audio;
mod brightness;
mod catalog_timing;
mod chan;
mod clipboard;
mod clipboard_os;
mod clock;
mod device_battery;
mod diagnostics;
mod dialog;
mod dock;
mod dock_menu;
mod dock_tuck;
mod edge;
mod error_log;
mod eyes;
mod graphics;
mod host;
mod hotkeys;
mod icons;
mod ipc;
mod keys;
mod layout;
mod limits;
mod limits_os;
mod media;
mod mic;
mod motion;
mod native_window;
mod picker_font;
mod pins;
mod places;
mod policy_config;
mod pomo;
mod power;
mod prayer;
mod prayer_os;
mod prayer_tab;
mod preview;
mod privacy;
mod quick;
mod remind;
mod remind_box;
mod settings;
mod settings_window;
mod shelf;
mod shelf_os;
mod sort;
mod stopwatch;
mod store;
mod system_os;
mod tabs;
mod taskbar;
mod text_settings;
mod theme;
mod timer;
mod toast;
mod topology;
mod water;
mod weather;
mod weather_os;
mod widgets;
mod window_catalog;
mod window_model;
mod window_source;

use windows::Win32::Foundation::E_INVALIDARG;
use windows::core::{Error, Result};

fn main() {
    error_log::install_panic_hook();
    if let Err(error) = run() {
        eprintln!("ROQA: {error}");
        error_log::startup_failure(error.code().0);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [] => host::run(host::Probe::None, None),
        #[cfg(debug_assertions)]
        [command] if command == "--probe-failure" => host::run(host::Probe::Initialization, None),
        #[cfg(debug_assertions)]
        [command] if command == "--probe-panic" => host::run(host::Probe::Callback, None),
        #[cfg(debug_assertions)]
        [command] if command == "--probe-device-loss" => host::run(host::Probe::DeviceLoss, None),
        [command, path] if command == "--diagnostics" => {
            host::run(host::Probe::None, Some(std::path::PathBuf::from(path)))
        }
        [command] => ipc::Command::parse(command)?.signal(),
        [command, ask, name] if command == "agent" && ask == "ask" => {
            agent::ask(name);
            Ok(())
        }
        [command, name, state] if command == "agent" => {
            let (Some(name), Some(state)) = (agent::clean(name), agent::State::parse(state)) else {
                return Err(Error::new(
                    E_INVALIDARG,
                    "Use: agent <name> <working|needs|done>.",
                ));
            };
            // A hook runs whether or not ROQA does: not running is not an error.
            let _ = ipc::Command::Agent.signal_after(|| {
                let _ = agent::write(name, agent::Report::State(state));
            });
            Ok(())
        }
        [command, pid, state] if command == "--taskbar" => taskbar::watch(pid, state),
        [command, name, value] if command == "progress" => {
            let (Some(name), Some(percent)) = (agent::clean(name), agent::percent(value)) else {
                return Err(Error::new(
                    E_INVALIDARG,
                    "Use: progress <name> <0-100|done>.",
                ));
            };
            let _ = ipc::Command::Agent.signal_after(|| {
                let _ = agent::write(name, agent::Report::Percent(percent));
            });
            Ok(())
        }
        [command, words @ ..] if command == "notify" && !words.is_empty() => {
            agent::notify(&words.join(" "));
            Ok(())
        }
        _ => Err(Error::new(
            E_INVALIDARG,
            "Expected no argument or one control command.",
        )),
    }
}
