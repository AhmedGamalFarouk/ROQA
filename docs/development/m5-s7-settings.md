# M5 S7: settings surface

Author: Claude Sonnet 5.5 (Tier 2), branch `feat/m5-s7-settings`, base `a967371`. REVIEW NOT RUN (paused by user 2026-11-11). Gate: `scripts/verify.ps1` PASS, **170 tests** (161 + 9). Release `roqa.exe` 990,208 B, SHA-256 `98369E6C2978D002D934522589F1E251E5B91E9719DB01D950A305E43A3A2549`; base 929,792 B, so **+60,416 B (+6.5 %)** (the window, `GetDriveTypeW`, `AdjustWindowRectExForDpi`). New `windows` feature `Win32_Storage_FileSystem`; no new crate.

## What changed
- `src/settings.rs` (pure): dock/notch on, per-monitor off list, timer sound, shelf cap `1..=12`, diagnostics report path (empty is off). `clamped` (at least one surface kind stays on; bounded single-line names and paths; pasted quotes stripped), `keep_one_monitor`, `surface_diff(old, new, devices)` (surfaces to create and destroy), `parse_timer` (`m`, `m:ss`, `h:mm:ss`, 1 s to 24 h).
- `src/store.rs`: `VERSION` 2, settings as `dock=`/`notch=`/`sound=`/`shelfcap=`/`off=`/`diag=` lines; a v1 file migrates to the defaults, a v2 one is clamped on load. Written with the existing debounce (one dirty mark per change).
- `src/settings_window.rs`: modeless standard-control window (checkboxes, edits, buttons) on the picker's `empty_dialog`/`control`/`dialog_proc` (small refactor in `picker.rs`). Created on `--show-settings`, destroyed on close. Checkboxes apply on click, edits on focus loss, Enter or Close; "Reset to defaults" re-applies `Settings::default()`. Text scale comes from the host's existing `TextSettings` (no second subscription).
- Start timer: a length edit plus "Start timer" calls `Timers::start` (the `allow(dead_code)` is gone); status line reports a 4th timer or a bad length. No `--timer` channel.
- `host.rs`: `apply_settings` creates and destroys only the diff's surfaces, trims the shelf (`Shelf::set_cap`), repoints the diagnostics report only when its path changed (a `--diagnostics` run keeps its own), marks the store dirty once. `place` skips a monitor that is off, so topology changes obey it. `ipc.rs`, `main.rs`, `run.ps1`: `--show-settings`.
- Fix from S6: `shelf::is_network` treats `DRIVE_REMOTE` (`shelf_os::drive_type`, local mount query) like UNC at activation: no stat on the host thread, open on its own thread.
- Idle: nothing resident when closed; open adds no timer or wake beyond its own messages.

## Tests (9)
Defaults and clamping; last-monitor guard; surface diff; reset; store lines and bad values; `parse_timer` (valid and 18 bad inputs); v1 migration and v2 clamp (`store`); shelf cap and drive classification (`shelf`).

## Defaults relied on (m5-preparation section 4)
Q5: the S7 list is right and launch-at-login stays out. Q6: proceed, review paused. Mine: both surface kinds cannot be off (else the host exits); the last connected monitor cannot be switched off; the shelf ceiling is the S6 constant 12; `--diagnostics <path>` beats the stored path for that run.

## NOT RUN
Opening the window, live apply (dock, notch, per monitor), a real timer start and its completion sound, reset, Enter/Escape/Tab traversal, text scale and DPI change while open, IME in the path edit, Narrator (S8), a real mapped drive.

## Deferred
Monitor list is fixed when the window opens (reopen after a hotplug); more than 8 monitors are not listed; an unreachable diagnostics path fails the catalog write like the CLI; a mapped-drive entry has no "network" UIA note; modal Ctrl+O and shelf entries 4 to 12 stay as in S6.
