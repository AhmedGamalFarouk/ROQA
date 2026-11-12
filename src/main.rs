#![windows_subsystem = "windows"]

// Keep existing diagnostic stderr output; the fallback records only fixed context, never arguments.
macro_rules! eprintln {
    ($message:literal $(, $argument:expr)* $(,)?) => {{
        std::eprintln!($message $(, $argument)*);
        crate::error_log::record($message);
    }};
}
mod audio;
mod brightness;
mod catalog_timing;
mod chan;
mod clock;
mod diagnostics;
mod dock;
mod dock_menu;
mod dock_tips;
mod edge;
mod error_log;
mod eyes;
mod graphics;
mod host;
mod icons;
mod ipc;
mod layout;
mod limits;
mod limits_os;
mod media;
mod motion;
mod native_window;
mod picker;
mod picker_font;
mod pins;
mod policy_config;
mod pomo;
mod power;
mod prayer;
mod prayer_os;
mod prayer_tab;
mod preview;
mod privacy;
mod settings;
mod settings_window;
mod shelf;
mod shelf_os;
mod sort;
mod store;
mod tabs;
mod text_settings;
mod theme;
mod timer;
mod topology;
mod water;
mod weather;
mod weather_os;
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
        _ => Err(Error::new(
            E_INVALIDARG,
            "Expected no argument or one control command.",
        )),
    }
}
