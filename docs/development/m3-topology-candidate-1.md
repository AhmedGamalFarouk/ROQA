# M3 topology candidate 1: per-monitor surfaces

Author: primary implementation agent (Claude Opus 5.5 High, user-requested top-level thread), 2026-11-10. Base HEAD ae6693645bf0434898d91784a2d41323e8246366. Uncommitted, no freeze. The milestone council is deferred by the user and accessibility checks are skipped (NOT RUN, not passed). Nothing here accepts M2 or M3.

## Change

- `src/topology.rs` (new) holds a pure model. `Monitor` has a GDI device name, which is a session-local identity, plus a work area and a DPI.
  - `settle` treats a snapshot as transient when it is empty, has a zero-size work area, a zero DPI, an empty name or duplicate names. It sorts monitors by device, so a change in enumeration order alone never triggers a rebuild.
  - `diff` returns the devices whose surfaces must be released (removed or changed) and the monitors that need new surfaces (added or changed).
- `src/native_window.rs`: `monitors()` makes one bounded pass.
  - `EnumDisplayMonitors` runs a callback that only pushes into a Vec with capacity reserved up front, capped at 16 monitors. It cannot reallocate or unwind.
  - `GetMonitorInfoW(MONITORINFOEXW)` is called outside the callback. HMONITORs are not kept after the call.
  - DPI comes from the existing hidden probe HWND, now placed on each monitor (`primary_dpi` was renamed `dpi_at`).
  - `primary_work_area` stays: the picker is still primary-only by design.
- `src/host.rs`: each surface is keyed by monitor device plus kind.
  - **Per-surface state.** Bounds, DPI, notch motion, intent and region, and dock hover/slots each moved into the surface. Each dock now has its own `dock_uia::Shared`. Hovering one monitor's notch or dock no longer drives the others.
  - **Show, hide and close.** These apply a kind to every monitor. The host exits when no kind is shown.
  - **Display changes.** DPI, display and settings messages are coalesced into one topology read on the UI thread. Unchanged monitors keep their HWNDs and motion. Removed or changed monitors release their surfaces, including the shadow and UIA providers.
  - **Retries.** A transient read, an enumeration error or a failed placement retries every 250 ms, at most 8 times per change. If retries run out and no surface is left, the error ends the host through the existing orderly shutdown. If other surfaces remain, the next display change or a Show command reads the topology again.
  - **Shared services.** The catalog/worker and IconService remain single. Icons are extracted at the densest dock DPI; a documented `shortcut:` lets lower-DPI docks resample them.
  - **Text scale.** A text-scale change now rebuilds only the notches.

## Verification (author; not approval)

- `scripts/verify.ps1 -EvidenceDirectory artifacts/m3-topology-1/verification`: checks PASS (fmt, clippy -D warnings, 27 tests, of which 5 are new topology tests) and `build --locked --release` PASS.
- Release binary `A822BF9CAE160F37B52109A15A1DFA465BFFAA751FA560A782FC952FC7CBB13B`. This rebuild replaced the earlier `21BBFE1A…C6` binary in `target/release`. The earlier native evidence belongs to that earlier binary.
- The model tests are deterministic and do not prove real enumeration, mixed-DPI placement or hotplug.
- Passive two-monitor run with `scripts/topology-check.ps1` on binary `A822BF9C…`: **PASS, 25 checks**. Evidence in `artifacts/m3-topology-1/native-1/`; the run is recorded in `progress.md`. Each monitor, including DISPLAY2 at negative x, has one correctly placed notch, shadow and dock, and both render. The foreground was unchanged. `--quit` exited 0 in 150 ms.
- A read-only screen query found two monitors on this machine: `\\.\DISPLAY1`, primary, 0,0 1920x1080, and `\\.\DISPLAY2`, -1920,0 1920x1080. The current binary will therefore create two notches and two docks here.

## Independent clean-code-guard (non-author, read-only, fresh tasks)

1. Review of `artifacts/m3-topology-1/inputs.sha256`, 24 entries, SHA-256 `0F223E15…050294`: **REVISE**. The path casing in that file is distorted by a `tr` bug; the reviewer verified it case-insensitively and it is preserved as reviewed.
   - **M3T1-CCG-MAJ-1:** a placement that keeps failing could leave an invisible resident that never recovers.
   - **MIN-1:** the minute could differ between notches.
   - **MIN-2:** each failing monitor consumed part of the retry budget.
   - **MIN-3:** `Surface` mixes notch-only and dock-only fields.
2. Author fixes, in `src/host.rs` only:
   - **MAJ-1:** `refresh_topology` and `retry_topology` now return errors when retries are exhausted and no surface is left. Show commands now request a topology read.
   - **MIN-1:** `place` refreshes the clock before painting a new notch.
   - **MIN-2:** the retry is scheduled once per pass.
   - **MIN-3:** deferred. The flat field mix existed in the former `Desktop`; splitting it into a per-kind role belongs to a later slice.
3. Fresh recheck of `artifacts/m3-topology-1/inputs-2.sha256`, 24 entries, SHA-256 `1E698C0FF0CBD2CE05B45282D1965F78B17BEAA9B85A0F922063D5D5F86E0BC1`: **ACCEPT**, code-only. All 24 hashes verified and check.ps1 ran with exit 0.
   - MAJ-1, MIN-1 and MIN-2 are resolved. The MIN-3 deferral was accepted. No new findings.
   - The recheck also observed, without raising a finding, that a Show command whose placement fails on any monitor still exits the host in an orderly way, while background topology reads tolerate the same failure.

## Open: runtime evidence

- **Existing native suites assume one notch and one dock.** `scripts/smoke.ps1` and the related suites look up surfaces with `FindWindow` by title. On this two-monitor machine they would see an arbitrary one of each, and their primary-work-area rectangle assertions may not hold. They have since been updated to follow the primary-monitor surface through `NativeProbe.Surfaces/PrimarySurface`, and absence checks now cover every monitor. The `DpiChange` case now asserts that an unchanged notch HWND stays at the real DPI. The updated suites compile and parse but have NOT been run on this binary.
- **Verified on hardware (passive run, single sample):** each monitor's notch and dock rectangle against that monitor's work area at 96 DPI, including DISPLAY2 at negative x; foreground preservation; graceful `--quit`.
- **Not yet verified on hardware:**
  - add, remove and reorder;
  - DPI and work-area change;
  - taskbar and work-area preservation;
  - failure cleanup;
  - per-monitor private bytes, handles and USER/GDI deltas (one two-monitor sample exists);
  - mixed DPI and any non-96 DPI.
  Each needs fresh desktop-free confirmation. Display changes need separate user authorization.
- **Later slices, still not started:** fullscreen suppression, previews, device loss, resume/Explorer restart, and a mixed-DPI icon cache per size.
- **Prior limits stay open:** Windows 10, physical IME, real non-1.0 text scale, broader RTL, Narrator, display-off, and compositor/GPU/DWM/power.
