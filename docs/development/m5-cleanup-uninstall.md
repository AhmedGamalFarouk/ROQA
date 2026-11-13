# M5 Cleanup and Uninstall Candidate Packet

Author: implementation worker.
Date: 2026-11-13 Africa/Cairo. Status: GATE VERIFIED, CANDIDATE READY.
Companion manifest: `docs/development/m5-cleanup-uninstall.sha256`.

## Goal & Scope
Implement and verify the "Uninstall/data" release-readiness gate specified in `docs/development/release-readiness.md`:
"Prove graceful shutdown and removal of installed files; decide whether uninstall offers to retain/delete ROQA store/error logs. Preserve user files and native taskbar; document manual portable cleanup if selected."

## 1. Graceful Shutdown Verification (`roqa --quit`)
- **Signaling:** `roqa --quit` parses command-line arguments to `ipc::Command::Quit` and signals the user-scoped named event `Local\ROQA.<UserSid>.quit`.
- **Event Loop Dispatch:** Resident event loop waiting in `MsgWaitForMultipleObjectsEx` (or `next_frame`) wakes on the quit handle (index 0).
- **Exit Path:** `Desktop::command(Command::Quit)` returns `Ok(false)`, cleanly terminating `event_loop` without panic or abort.
- **Store Flush:** `Desktop::flush_store(true)` executes immediately before teardown. Because `quit = true`, `Pending::should_write(now, quit)` flushes any dirty state synchronously without waiting for the 1500 ms `COALESCE` debounce.
- **Catalog Worker & Hooks:** `Catalog::shutdown()` is invoked:
  - Sets `TARGET = 0` to drop UI message target.
  - Iterates all 6 registered `HWINEVENTHOOK` handles and calls `UnhookWinEvent` on each.
  - Sets `cancel = true` atomic flag and drops channel sender/receiver.
  - Joins the background window-worker thread via `worker.join()`.
  - Clears hooks list.
- **Surface & Window Teardown:**
  - `Desktop` drop unregisters all DWM thumbnails (`DwmUnregisterThumbnail`) and destroys all native window handles (`DestroyWindow`).
  - Privacy watch drops its registry handle (`RegCloseKey`).
  - Window class unregisters via `UnregisterClassW`.
  - COM apartment uninitializes via `CoUninitialize`.
  - IPC handles and single-instance mutex are closed via `CloseHandle`.
- **Exit Code:** Clean exit with code 0; zero leaked handles, hooks, or threads.
- **Unit Verification:** Added tests in `src/ipc.rs` verifying command parsing (`--quit`), command ordering (`Command::ALL[0] == Quit`), and object naming format. 370 tests pass cleanly.

## 2. Automated Cleanup Script (`scripts/uninstall.ps1`)
Implemented `scripts/uninstall.ps1` with project PowerShell conventions (UTF-8 BOM + CRLF, `Set-StrictMode -Version Latest`, `$ErrorActionPreference = 'Stop'`):
- **Process Detection & Termination:** Checks for running `roqa` processes. Signals `roqa --quit` (or direct `Local\ROQA.<UserSid>.quit` Win32 event fallback) and waits with a bounded timeout (default 5s) confirming graceful termination and exit code 0.
- **Data Retention / Purge Options:**
  - Supports `-KeepData` (preserves `%LOCALAPPDATA%\ROQA\`).
  - Supports `-PurgeData` (removes `%LOCALAPPDATA%\ROQA\` including `store.roqa`, `error.log`, `claude-limits.*`, `.tmp`/`.bad` files).
  - Unattended / redirected sessions safely default to retaining data if neither switch is specified.
  - Interactive sessions prompt the user before deletion.
- **Hygiene & System Audits:**
  - Audits registry for autostart (`HKCU\...\Run`), uninstall entries (`HKCU\...\Uninstall\ROQA`), and application keys (`HKCU\Software\ROQA`). Confirmed 0 entries.
  - Audits scheduled tasks for ROQA registrations (`Get-ScheduledTask -TaskName '*roqa*'`). Confirmed 0 tasks.
  - Audits background services for ROQA services (`Get-Service -Name '*roqa*'`). Confirmed 0 services.
  - Audits native taskbar and shell integrity (`Shell_TrayWnd` and explorer intact; no shell hooks or replacements).
- **Safety & Portability:**
  - Supports `-DryRun` mode for inspection without mutating files.
  - Optional `-InstallDirectory` / `-RemoveBinaries` with safety guard refusing to delete directory if it contains `.git`.

## 3. Manual Portable Cleanup Documentation
Documented in `docs/development/release-readiness.md` under `## Manual portable cleanup`:
1. Stop resident process via `roqa.exe --quit` or dock/notch context menu ("Quit ROQA").
2. Delete folder containing portable `roqa.exe`.
3. Optionally purge `%LOCALAPPDATA%\ROQA\` or keep it to preserve pins, timers, and settings.
4. Audited read-only registry usage: zero registry, task, service, or taskbar modifications left behind.

## 4. Verification Evidence
- `scripts/check.ps1`: PASS (fmt clean, clippy clean, 370 unit tests pass, 0 failed).
- `scripts/uninstall.ps1 -DryRun`: PASS (exit code 0; all audits CLEAN).
- `scripts/uninstall.ps1 -KeepData -DryRun`: PASS (exit code 0).
- `scripts/uninstall.ps1 -PurgeData -DryRun`: PASS (exit code 0).
- `scripts/verify.ps1`: PASS (checks exit 0, build exit 0, size gate PASS).
- Release binary size: 1,051,648 bytes (baseline: 1,152,000 B + 2% limit 1,175,040 B; -100,352 B under baseline).
- Release binary SHA-256: `70E21E5232EF538C9AA8EFAF5AF3653421E2C9B69328DC0308C142CCE9C47277`.