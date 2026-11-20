<div align="center">
  <img src="assets/icon.svg" width="72" height="72" alt="ROQA Logo">
  <h1>ROQA</h1>
  <p><strong>A fast, ultra-lightweight native Windows dock and dynamic notch island built in Rust.</strong></p>
  <p>Pure Win32 &bull; DirectComposition &bull; Direct2D &bull; 0% WebView &bull; Single ~1.5 MB Binary</p>
</div>

---

## Overview

**ROQA** is a modern desktop enhancement shell for Windows 11 built entirely in native Rust. It delivers a fluid, interactive top dynamic notch island and bottom application dock with zero runtime bloat.

- **Ultra-lightweight:** Self-contained single-binary executable (~1.5 MB) compiled with full LTO and stripped debug info.
- **Hardware-Accelerated DirectComposition:** Sub-pixel Direct2D and DirectWrite rendering with smooth 60fps springs and transitions.
- **Event-Driven Win32:** No keyboard hook and no polling while idle: it sleeps until Windows reports something, with a few timed waits (the clock each minute, a window-list recheck every 30 s) and short ones only while something moves.
- **Non-Invasive:** Hides the Windows taskbar only while it runs (a Settings switch) and puts it back exactly as it was, even after a crash; registers no background services, and makes zero unsolicited startup or registry modifications.
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
- **Live App Previews:** Hover a running app to preview its latest window; click the thumbnail to switch, or its close button to request a normal close with unsaved-work prompts preserved.
- **Dock Context Menu:** Clean right-click menu with options to launch new instances, pin/unpin apps, and close windows.
- **Notification Badges:** Real-time unread counter and status indicator dots on application icons.
- **Desktop Strip:** Quick show-desktop strip.

### Personalization & Settings
- **Theme Palette:** Five themes sharing one shape, icon and motion system: *Classic, Light, Midnight, Frosted tint* and *Liquid glass*, with a single warm accent (Lamp `#FFB454`).
- **Windows System Accent Match:** Optional integration with Windows system accent colour (`HKCU\...\DWM\AccentColor`).
- **Tuning:** Adjustable notch sizing percentage, animation pace, and click-to-open behavior.

---

## Architecture & Technology Stack

| Component | Implementation |
|---|---|
| **Language** | Rust 2024 edition (1.91.1+) |
| **Graphics** | Direct2D 1.1, DirectWrite, WIC, DirectComposition via Windows SDK |
| **Windowing & Events** | Pure Win32 (`CreateWindowExW`, `SetWinEventHook`) |
| **Media Integration** | WinRT GSMTC (`Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager`) |
| **Audio Endpoints** | Windows Core Audio APIs (`IMMDeviceEnumerator`, `IAudioEndpointVolume`) |
| **Weather & Networking** | Windows WinHTTP (`winhttp.dll`) with OS-managed TLS stack |
| **Storage & Settings** | One `key=value` text file, written atomically: `%LOCALAPPDATA%\ROQA\store.roqa` |

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
roqa --show-settings     # Open the settings panel

# Diagnostics & Shutdown
roqa --diagnostics <out> # Start ROQA with counters written to <out> (only when none runs)
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

# Gracefully terminate process and purge application data (%LOCALAPPDATA%\ROQA)
./scripts/uninstall.ps1 -PurgeData

# Or terminate process while preserving settings
./scripts/uninstall.ps1 -KeepData
```

ROQA leaves zero background services, zero scheduled tasks, and zero orphaned registry keys. Agent hooks added in Settings stay in the agents' own files: press Undo there first (the script lists any that remain).
