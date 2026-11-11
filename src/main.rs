#![windows_subsystem = "windows"]

mod audio;
mod catalog_timing;
mod clock;
mod dock;
mod dock_uia;
mod graphics;
mod host;
mod icons;
mod ipc;
mod layout;
mod media;
mod motion;
mod native_window;
mod notch_uia;
mod picker;
mod picker_font;
mod store;
mod text_settings;
mod topology;
mod window_catalog;
mod window_model;
mod window_source;

use windows::Win32::Foundation::E_INVALIDARG;
use windows::core::{Error, Result};

fn main() {
    if let Err(error) = run() {
        eprintln!("ROQA: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [] => host::run(host::Probe::None, None),
        [command] if command == "--probe-failure" => host::run(host::Probe::Initialization, None),
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
