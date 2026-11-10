# M3 device-loss candidate 1: graphics device-loss recovery

- **Author.** A Claude Opus 5.5 High worker, launched by the ROQA Chief Thread at the user's request ("do 3": the next code slice). The author approves nothing.
- **Base.** `6f665cf` (origin/main), branch `feat/m3-device-loss`. Uncommitted; not frozen for milestone acceptance.
- **Scope.** PLAN.md M3 acceptance: "inject graphics-device-loss/recreation, validating retained-state recovery or a clearly reported safe failure". Detection, rebuild and bounded retry, all on the existing UI thread.
- **Changed files.**
  - `src/graphics.rs` (+67/-13): named fields; device-loss classification and query; one test.
  - `src/host.rs` (+188/-17): the `Rebuild` schedule, the recovery paths and the debug probe; two tests.
  - `src/main.rs` (+2): the debug-only probe argument.
- **Final manifest.** `artifacts/m3-device-loss-1/round-2/inputs.sha256`, 10 entries, SHA-256 `DC339882F7A8063094A00125A22BE3B7717223F38CA5435C8EE1460BE4BB2DCF`. The round-2 brief said "11 entries" by mistake; the reviewer confirmed the 10 entries and the file's own hash.

## What actually signals a loss (verified)

**MS Learn**, read during this slice:

- **`IDCompositionDevice::CheckDeviceState`:**
  - "If the Microsoft DirectX Graphics Infrastructure (DXGI) device is lost, the DirectComposition device associated with the DXGI device is also lost."
  - "When it detects a lost device, DirectComposition sends the WM_PAINT message to all windows that are composing DirectComposition content using the lost device."
  - "An application should call CheckDeviceState in response to each WM_PAINT message."
  - Recovery: "creating new DXGI and DirectComposition devices, and recreating all content. (It's not possible to create just a new DXGI device and associate it with the existing DirectComposition device.)"
- **`DCompositionCreateDevice2`:** the iid may be `IDCompositionDevice` or `IDCompositionDesktopDevice`. The object is a v2 desktop device, and you can QueryInterface between them. The round-1 reviewer's headless probe confirmed the cast and `CheckDeviceState` == TRUE on this machine.
- **`ID3D11Device::GetDeviceRemovedReason`:** returns S_OK, `DXGI_ERROR_DEVICE_HUNG`, `_REMOVED`, `_RESET`, `_DRIVER_INTERNAL_ERROR` or `_INVALID_CALL`.
- **"Handle device removed scenarios in Direct3D 11":** `DXGI_ERROR_DEVICE_REMOVED` or `DXGI_ERROR_DEVICE_RESET` means "reinitialize the Direct3D device and recreate any device-dependent resources".
- **`IDCompositionSurface::BeginDraw`:** the page names no specific device-loss return value, only "an HRESULT error code". So classification by code alone is not enough, and the device is also queried.
- **`CreateTargetForHwnd`:** "If a composition target is already bound to the specified window at the specified layer, this method fails." The old target must be released before a new one binds that HWND.

**Windows crate `=0.62.2`:**

- `ID3D11Device::GetDeviceRemovedReason() -> Result<()>`.
- `IDCompositionDevice::CheckDeviceState() -> Result<BOOL>`. It is only on v1 `IDCompositionDevice`, not on `IDCompositionDevice2` or the desktop device.
- `DXGI_ERROR_*` are in `Win32::Graphics::Dxgi`. `D2DERR_RECREATE_TARGET` is in `Win32::Foundation`.
- All are already enabled features; there is no new crate feature.

## Which objects depend on the device

- **Device-dependent, all owned by `Graphics` and `Canvas`:**
  - the D3D11, D2D and DComp devices;
  - per surface and per notch shadow: the `IDCompositionTarget` on the HWND, the visual and the `IDCompositionSurface`.
  - Brushes, geometries, command lists, effects, text layouts and icon bitmaps are created inside each draw and released at its end.
- **There is no cached device-dependent bitmap.** `draw_slot` rebuilds the icon bitmap per paint from the CPU `IconPixels` (an existing `shortcut:` comment).
- **What survives:**
  - `IconPixels` and the icon cache (CPU memory);
  - the picker (GDI);
  - the catalog, hooks and text settings.

## What ROQA did on loss before this change (evidence: code at `6f665cf`)

- Every paint goes `Canvas::draw` → `BeginDraw`/`EndDraw`/`Commit` with `?`. Those calls are in `refresh_clock`, `animate`, `refresh_dock` (including `Canvas::resize`), `refresh_fullscreen` and `place`/`show`.
- That chain reaches `event_loop`'s `?`, then `host::run`, then `main`, which prints the error and calls `exit(1)`.
- **A loss therefore ended ROQA.** It is an orderly exit: no panic, no spin, and the native taskbar is untouched. But there was no recovery.
- WM_PAINT was only validated (`native_window::dispatch`), so a loss while idle went unnoticed until the next paint failed. That paint is a minute tick, a catalog change or hover.
- `refresh_topology` turned a placement failure, including a loss, into a topology retry. It dropped that monitor for up to 8 × 250 ms, then exited if no surface was left.

## Change

### `src/graphics.rs`
- **`Graphics { composition, d3d }`.** It keeps the `ID3D11Device` so that removal can be queried.
- **`lost(&Error)`** is `is_loss(code) || removed()`.
- **`is_loss`** is true for:
  - `DXGI_ERROR_DEVICE_REMOVED`, `_RESET`, `_HUNG` and `_DRIVER_INTERNAL_ERROR`;
  - `D2DERR_RECREATE_TARGET`.
- **`removed()`** is `GetDeviceRemovedReason().is_err()`, or a successful QI to `IDCompositionDevice` whose `CheckDeviceState` reports invalid. A failed QI is not treated as a loss.
- **Test:** `only_device_loss_results_trigger_a_rebuild`.

### `src/host.rs`
- **`Desktop::update`.** The existing per-pass refreshes move here unchanged: topology, text, dock, fullscreen, clock, picker and animate.
- **Detection, both with zero idle cost:**
  - **Paint result:** `survive(result, fallback)` runs on `update` and on the Show command results. A loss calls `lose_graphics()` and returns the fallback. A non-loss error still ends the host, as before.
  - **Existing event:** `WM_PAINT` on a surface window checks `graphics.removed()`. DComp sends this message on loss. Our `SetWindowRgn` uses redraw=false, so WM_PAINT is rare otherwise.
- **`lose_graphics`.** It drops every surface at once, which releases all old-device targets, visuals, surfaces, HWNDs and UIA providers. It then schedules the rebuild.
- **`Rebuild` value type** (`due`, `attempts`, `started`), with no GPU dependency:
  - **Delay.** 250 ms doubling to an 8 s cap.
  - **Pending.** One rebuild stays pending across repeated detections.
  - **Budget.** A failure ends the host once 8 rebuilds have run with no settled 60 s between losses.
  - **Flapping.** A device that keeps failing after successful rebuilds is limited to one rebuild per 8 s.
  - **Failure handling.** Any rebuild failure retries, because one during a driver reset need not read as a loss (NIT-2, kept on purpose).
- **`refresh_graphics`** runs at the loop top when due:
  - It clears the surfaces, creates a new `Graphics` and assigns it, which drops the old device after its surfaces and before any new surface exists.
  - It re-shows every shown kind through the existing `show`/`place` path, which paints once, honours fullscreen hiding and shows after the first paint.
  - On success it calls `request_topology()` and sets `clock_due = now`, because ROQA was windowless and may have missed display, resume, TaskbarCreated or time broadcasts. A monitor dropped by a failed placement is placed again by that re-read.
  - The loop wait includes `rebuild.wait(now)`.
- **Debug-only probe `--probe-device-loss`.** `#[cfg(debug_assertions)]` is on the `Probe` variant, on the `main.rs` arm and on the `run()` block. After startup it feeds one synthetic `DXGI_ERROR_DEVICE_REMOVED` through `survive`. The round-2 reviewer confirmed it is absent from release builds with release-profile clippy.
- **Tests:**
  - `graphics_rebuilds_back_off_to_a_cap`;
  - `rebuilds_keep_one_pending_spend_a_budget_and_refresh_once_settled`.

### What was not added, and why

- **No `CheckDeviceState` timer or poll, no thread, no new crate feature.**
- **No surface-factory or retained visual tree.** MS Learn rules it out: a new DXGI device cannot be attached to the existing DComp device.
- **No in-place canvas rebuild.** Rebuilding whole surfaces reuses the tested topology path. It also guarantees that no HWND keeps an old composition target.
  - Cost: HWND and UIA dock elements are recreated, and hover and motion reset. This is the same as a topology change.
- **No REPOSITION-time device check.** DComp's WM_PAINT already covers idle loss.

### Preserved behaviour

These are checked by both reviewers by reading; none are natively verified:
- keyboard/UIA (providers recreated with the dock);
- always-animate;
- fullscreen hiding (`place` honours `self.fullscreen`);
- hidden-surface paint skipping;
- the clock deadline;
- `REPOSITION` on resume and TaskbarCreated.

Deferred nits M3SR-OPUS-NIT-1 and -2 stay deferred, because the role comments and `Surface::create` were not touched.

### Footprint

- Idle: zero added wakes.
- Per WM_PAINT on a surface: one `GetDeviceRemovedReason`, one QI and one `CheckDeviceState`.
- Memory: one extra `ID3D11Device` reference per `Graphics` clone, and three small fields.
- During a rebuild, old and new devices never coexist with surfaces: the old surfaces are gone before the new device exists, and the old device is dropped at assignment.

## Verification (author, pinned `scripts/verify.ps1`)

| Round | Evidence | Result | Release `roqa.exe` SHA-256 |
| --- | --- | --- | --- |
| pre-review | `artifacts/m3-device-loss-1/verification/` | PASS, 32 tests | `6FCD52BEA9ECEC5488E42473A6B954BC0E64C40D0F0951A7E2F2E3E08F75FCE7` |
| 1 | `.../verification-r1/` | PASS, 32 tests (comment fix) | `E597FF1F474BE964F42B5DE6E83D8D177FBE3B6758EFFDA4D5E09581566372A6` |
| 2 (final) | `.../verification-r2/` | PASS: fmt, clippy -D warnings, **33 passed, 0 failed**, locked release build | `2D28A19A2B6D25004EBBCD169E212CBC86C2E139271ED70A831C14CBD04C691C` |

- **Pre-checks.** Before every cargo run, no `roqa.exe` and no `measure-idle` PowerShell was running.
- **Toolchain.** `.tools` is a git-ignored junction to the root checkout's pinned toolchain. Nothing was downloaded, and PATH is unchanged.
- **Line endings.** `src/graphics.rs`, `src/host.rs` and `src/main.rs` are LF on disk, and the index is LF (`core.autocrlf=true`). Manifest hashes are of the bytes on disk.

## Independent review (Opus 5.5 High, clean-code-guard, read-only, a fresh task each round)

| Round | Manifest SHA-256 | Verdict | Report | Task |
| --- | --- | --- | --- | --- |
| 1 | `931862F2A952E4052FB245E778481F55C1CF19974FFC91213EB784F8B765F5A0` | CHANGES REQUIRED (2 MAJOR, 2 MINOR, 2 NIT) | `artifacts/m3-device-loss-1/round-1/review-opus-1.md` | <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Add7bf65f-8b71-4290-b537-86d4a10b6860%3Adelegate-task%3A2bd768ae-18f9-458e-800a-d88552108ba3> |
| 2 | `DC339882F7A8063094A00125A22BE3B7717223F38CA5435C8EE1460BE4BB2DCF` | **ACCEPT WITH NITS** (3 NIT) | `artifacts/m3-device-loss-1/round-2/review-opus-2.md` | <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Add7bf65f-8b71-4290-b537-86d4a10b6860%3Adelegate-task%3Aae5d54c2-afcf-4253-b6cb-da77ebd75f56> |

What both reviewers did:
- Each verified the manifest at start and end, and checked that `git diff` equals `candidate.diff`.
- Each ran `scripts/check.ps1`: exit 0, with 32 tests in round 1 and 33 in round 2.
- Round 2 also ran release-profile clippy (exit 0) and matched the on-disk release hash, but did not rebuild it.

Sol was not used, per the user's budget.

### Findings and dispositions

- **M3DL-OPUS-R1-MAJOR-1.** A monitor dropped by a topology placement that failed on the lost device was not re-placed by the rebuild. **Fixed:** `request_topology()` now runs after a successful rebuild. Round 2: resolved.
- **M3DL-OPUS-R1-MAJOR-2.** Broadcasts were missed while ROQA was windowless. **Fixed:** topology re-read and `clock_due = now`. Round 2: resolved.
- **M3DL-OPUS-R1-MINOR-1.** There was no injection path. **Fixed:** the debug-only `--probe-device-loss`, with zero release cost. Round 2: resolved in code; the native run is still outstanding.
- **M3DL-OPUS-R1-MINOR-2.** The bookkeeping was untested. **Fixed:** the `Rebuild` value type and its test. Round 2: resolved.
- **M3DL-OPUS-R1-NIT-1.** Tuple fields. **Fixed:** named fields.
- **M3DL-OPUS-R1-NIT-2.** Non-loss rebuild failures retry. **Kept and documented** on the constants. Round 2: accepted.
- **M3DL-OPUS-R2-NIT-1.** A successful rebuild and a WM_PAINT detection are not logged. **Deferred:** any edit needs another review round. The native plan observes recovery through HWND changes instead of stderr. Recommended for the next candidate that touches `host.rs`.
- **M3DL-OPUS-R2-NIT-2.** `Rebuild::finish(bool)` takes a flag argument and both changes state and answers a question. **Deferred** as a style nit; the test covers both outcomes.
- **M3DL-OPUS-R2-NIT-3.** `retry_topology` can end the host when there are no surfaces only because a rebuild is pending. **Deferred.** It needs the old device to report `removed() == false` while the rebuild is delayed 2 s or more and 8 placements fail. Recorded as a known edge.

## Native evidence plan (NOT RUN)

General conditions:
- Each case needs a **fresh desktop-free confirmation from the user**, plus the existing read-only pre-check: foreground, input-idle, and no ROQA, cargo or measurement process.
- Each case ends with a graceful `roqa --quit`.
- Evidence goes under `artifacts/m3-device-loss-1/native-*`.
- Before and after each case, record:
  - per monitor: the HWND, rect, visible and topmost state of every ROQA window;
  - the clock text;
  - private bytes, kernel handles, and GDI/USER counts;
  - stderr.

1. **In-process injection (safe; debug build only).**
   - Build the debug binary with `scripts/cargo.ps1 build --locked` and record its SHA-256. It is not the release binary.
   - Run `roqa --probe-device-loss`.
   - Pass criteria:
     - The process stays alive.
     - stderr contains exactly one `Graphics device lost:` line and no `Graphics rebuild:` line.
     - Within 1 s, every shown surface exists again on every monitor, with **new HWNDs**, at the baseline rects, visible and topmost.
     - Notch clock text is correct and the dock items match the baseline.
     - The UIA dock tree is present again (Inspect or Accessibility Insights).
     - After 60 s idle, handles, GDI and USER counts are within the PLAN plateau tolerances of a non-probe debug run (≤10 handles, ≤5 GDI/USER).
     - `--quit` exits 0.
   - **Variant A:** run with a full-screen app in the foreground on one monitor. After the rebuild, that monitor's surfaces stay hidden and are restored, painted, when full screen ends.
   - **Variant B:** run with the notch or dock hidden by IPC (`--hide-dock`). Only the shown kinds are rebuilt.
2. **Real TDR (`dxcap -forcetdr`, or a driver or GPU toggle).**
   - This is **system-affecting**: it resets the GPU for every application. It **needs separate explicit user authorization** beyond the desktop-free confirmation, and the worker must not do it unasked.
   - Run on the release binary `2D28A19A…C691C`, or on a rebuild whose hash is recorded.
   - Pass criteria:
     - ROQA keeps running.
     - Either recovery completes within 32 s with the criteria of case 1 (new HWNDs, correct rects, clock, dock and UIA, no leak), or, if every rebuild fails, ROQA exits non-zero with a `Graphics rebuild:` error. That is the safe, visible failure.
     - The native taskbar is untouched throughout.
     - No CPU spin: process CPU under 1 % averaged over the 30 s after recovery.
   - Record which detection fired: a paint error, or WM_PAINT seen through the absence of a preceding paint.
3. **Remote session / GPU switch** (for example, RDP into the machine, then back to the console). This is **session-affecting** and needs separate explicit user authorization. The pass criteria are the same as case 2.
4. **Regression.** These existing suites rerun on the final binary with no new failures:
   - `smoke.ps1` (including `--probe-failure` and `--probe-panic`);
   - `topology-check.ps1`;
   - `picker-smoke.ps1`;
   - `catalog-smoke.ps1`;
   - `packaged-hung-check.ps1`.
   - Also take one 30 s `measure-idle.ps1` sample to confirm no added idle wakes. Below the measurement floor, report it as inconclusive.

## Limits and unknowns

- **NOT RUN:**
  - every native case above, including the probe;
  - idle and trace measurement;
  - a reviewer rebuild of the release binary;
  - the council (Sol High plus Opus High);
  - accessibility;
  - Windows 10.
- **UNVERIFIED:**
  - Which HRESULT `BeginDraw`/`EndDraw`/`Commit` actually return after a real TDR. This is covered either way by the `removed()` query.
  - Whether DWM restart or remote-session changes surface as `CheckDeviceState == FALSE`.
  - WM_PAINT delivery timing after a loss.
- **Known edges:**
  - R2-NIT-3 above.
  - Recovery recreates HWNDs, so a screen reader focused on a dock item loses its focus, the same as on a topology change.
  - The debug probe exercises classification and rebuild, not real detection.
