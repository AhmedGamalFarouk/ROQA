<div align="center">
  <img src="assets/icon.svg" width="72" height="72" alt="ROQA Logo">
  <h1>ROQA</h1>
  <p><strong>A fast, ultra-lightweight native Windows dock and dynamic notch island built in Rust.</strong></p>
  <p>Pure Win32 &bull; DirectComposition &bull; Direct2D &bull; 0% WebView &bull; Single ~1 MB Binary</p>
</div>

---

## Overview

**ROQA** is a modern desktop enhancement shell for Windows 11 built entirely in native Rust. It delivers a fluid, interactive top dynamic notch island and bottom application dock with zero runtime bloat.

- **Ultra-lightweight:** Self-contained single-binary executable (~1.08 MB) compiled with full LTO and stripped debug info.
- **Hardware-Accelerated DirectComposition:** Sub-pixel Direct2D and DirectWrite rendering with smooth 60fps springs and transitions.
- **Pure Event-Driven Win32:** Zero polling loops or invasive system hooks; sleeps quietly when idle.
- **Non-Invasive:** Leaves the native Windows taskbar intact, registers no background services, and makes zero unsolicited startup or registry modifications.
- **Multi-Monitor & DPI-Native:** Full Per-Monitor V2 (PMv2) DPI awareness and automatic fullscreen suppression (auto-hides during gaming and fullscreen video).

---

## Features

### Dynamic Notch Island & Shelf
- **Interactive Top Capsule:** Displays time and date, expanding on hover to reveal rich modular tabs and tools.
- **Media Island:** Full Windows Global System Media Transport Controls (GSMTC) integration with album artwork, title/artist display, playback controls, scrubbable progress bar, and volume slider with quick audio device switcher.
- **Modular Widgets:** Week calendar view, live battery status pill, and power notifications.
- **Live Weather:** Real-time conditions, temperature, and hourly forecast powered by Open-Meteo with automatic geolocation or manual city search.
- **Astronomical Prayer Times:** High-precision prayer calculations (Fajr, Dhuhr, Asr, Maghrib, Isha) supporting major calculation authorities (Egyptian General Authority, ISNA, MWL, Umm al-Qura, Karachi), juristic methods (Shafi'i/Hanafi), and audio alerts.
- **Pomodoro & Quick Timers:** Preset countdowns (25m, 15m, 5m) or custom durations with visual progress and completion chime.
- **Drop-Target File Shelf:** Drag-and-drop quick-staging drawer for files, folders, and snippets with shell integration.
- **AI Usage Monitor:** Live tracking of tool subscription usage (Antigravity, Codex, etc.).
- **Interactive Mascot:** Expressive reactive eyes with glance intent, wake animations, and customizable mood presets.

### Application Dock
- **Window Grouping & Catalog:** Tracks running windows by AppUserModelID and executable path via native out-of-context win-events.
- **Shell Icons:** Asynchronous background icon extraction with bounded memory caching.
- **Pin Management:** Pinned applications with drag-and-drop reordering, middle-click/context-menu launch.
- **Window Switching & Minimizing:** Single-click activation, window cycling within groups, and minimizing.
- **Dock Context Menu:** Clean right-click menu with options to launch new instances, pin/unpin apps, and close windows.
- **Notification Badges:** Real-time unread counter and status indicator dots on application icons.
- **Desktop & Minimized Strip:** Quick show-desktop strip and minimized window drawer.
- **Window Picker:** Modeless search dialog (`--show-picker`) to instantly filter and jump to any active window.

### Personalization & Settings
- **Theme Palette:** Built-in curated themes including *Dark, Light, OLED, Sunset, Midnight, Matcha, Rose, Cyber,* and *Frosted Glass*.
- **Windows System Accent Match:** Optional integration with Windows system accent colour (`HKCU\...\DWM\AccentColor`).
- **Global Hotkeys:** Opt-in shortcuts (`RegisterHotKey`) for toggling the window picker and switching audio output devices.
- **Tuning:** Adjustable notch sizing percentage, animation pace, and click-to-open behavior.

---

## Architecture & Technology Stack

| Component | Implementation |
|---|---|
| **Language** | Rust 2024 edition (1.91.1+) |
| **Graphics** | Direct2D 1.1, DirectWrite, WIC, DirectComposition via Windows SDK |
| **Windowing & Events** | Pure Win32 (`CreateWindowExW`, `SetWinEventHook`, `RegisterHotKey`) |
| **Media Integration** | WinRT GSMTC (`Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager`) |
| **Audio Endpoints** | Windows Core Audio APIs (`IMMDeviceEnumerator`, `IAudioEndpointVolume`) |
| **Weather & Networking** | Windows WinHTTP (`winhttp.dll`) with OS-managed TLS stack |
| **Storage & Settings** | Atomic JSON persistence under `%APPDATA%\ROQA\` |

---

## Developing & Building

### Prerequisites
- **OS:** Windows 11 x64 (Build 22000+)
- **Build Tools:** Visual Studio 2022 C++ Build Tools with Windows 10/11 SDK installed.

### Setup & Bootstrap
ROQA uses a self-contained local toolchain managed via PowerShell scripts (avoids polluting global `PATH`):

```powershell
# 1. Bootstrap local Rust toolchain and tools
./scripts/bootstrap.ps1

# 2. Run formatting, Clippy, unit tests, and release build verification
./scripts/verify.ps1

# 3. Launch ROQA
./scripts/run.ps1
```

### Cargo Wrapper
To run Cargo commands through the pinned local toolchain:

```powershell
./scripts/cargo.ps1 check --all-targets
./scripts/cargo.ps1 test
./scripts/cargo.ps1 clippy --locked --all-targets -- -D warnings
```

---

## CLI & IPC Controls

When ROQA is running, you can control its surfaces and features via command-line arguments:

```powershell
# IPC surface control
roqa --show-notch        # Show the notch island
roqa --hide-notch        # Hide the notch island
roqa --show-dock         # Show the dock
roqa --hide-dock         # Hide the dock

# UI dialogs
roqa --show-picker       # Open the window search picker
roqa --show-settings     # Open the settings panel

# Diagnostics & Shutdown
roqa --diagnostics <out> # Output atomic catalog snapshot
roqa --quit              # Gracefully shut down ROQA and persist settings
```

---

## Verification & Testing

ROQA includes automated native verification scripts:

```powershell
# Comprehensive suite (checks, clippy, unit tests, release build, size ratchet)
./scripts/verify.ps1

# Interactive native smoke test (tests surface composition, DPI, focus preservation)
./scripts/smoke.ps1

# 100-cycle idle resource stability sampler
./scripts/measure-idle.ps1

# Window catalog event resilience and burst test
./scripts/catalog-smoke.ps1
```

---

## Uninstall & Clean Removal

ROQA is entirely portable. To remove it cleanly:

```powershell
# Preview what would be removed
./scripts/uninstall.ps1 -DryRun

# Gracefully terminate process and purge application data (%APPDATA%\ROQA)
./scripts/uninstall.ps1 -PurgeData

# Or terminate process while preserving settings
./scripts/uninstall.ps1 -KeepData
```

ROQA leaves zero background services, zero scheduled tasks, and zero orphaned registry keys.
