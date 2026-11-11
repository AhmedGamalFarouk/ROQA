# M5 production-readiness assessment (2026-11-11 Africa/Cairo)

Scope: read-only assessment of root `main` `238c2c8` plus the uncommitted smoke fix. Author: Claude Opus 5.5 (medium). gstack 1.99.0.0: `/health` and `/review` critical-pass rubrics applied by hand (Rust has no web/browser surface; verify.ps1 already covers fmt/clippy/test). Not a review, not an acceptance. No code changed, no native/GUI run, nothing committed.

Evidence base: binary 999,424 B SHA `A329BF01...622C`; verify 175 tests PASS; topology 25 PASS; smoke 331 PASS (uncommitted harness fix). Everything below marked *static* comes from reading code, not from a run.

## Production blockers

| ID | Finding | Location | Evidence |
|---|---|---|---|
| B1 | **UIA teardown order.** `Surface::drop` runs before its fields drop, so `notch_uia::invalidate` sets `hwnd = 0` before `Role::Notch.accessible` (`Accessibility::drop`) calls `UiaDisconnectProvider`. The hosted root then cannot resolve; 222 cleanup calls in 110 smoke cycles logged `E_INVALIDARG`. Leak or resource harm unproven. | `src/host.rs:163-170`, `src/dock_uia.rs:387-402`, `src/notch_uia.rs:60` | smoke log (m5-native-sol-2.md); order confirmed static |
| B2 | **High contrast ignored.** No `SPI_GETHIGHCONTRAST`/system-colour path; all four themes paint fixed palettes. | `src/theme.rs`, `src/graphics.rs` | static grep: no matches |
| B3 | **Keyboard/Narrator gaps in M5 features.** Shelf entries have no keyboard focus; pin reorder has no keyboard path (S8 never landed); shelf entries 4-12 are stored but unreachable (`VISIBLE = 3`, cap 1-12 in settings). | `src/shelf.rs:9-11`, S4/S6 packets | static + deferred lists |
| B4 | **Errors are invisible.** `windows_subsystem = "windows"` drops every `eprintln!` (startup failure, cleanup errors) unless `--diagnostics` is given. A failed launch exits 1 with no message. | `src/main.rs:1,37-41` | static |
| B5 | **No release path.** License, signing, installer/uninstall and update plan are open (PLAN M6, PRODUCT). | PLAN.md, PRODUCT.md | docs |
| B6 | **No accepted milestone since M3 and no M5 performance proof.** Reviews paused; M4 and M5 slices NOT accepted; M5 budget gates (≤80 MiB, ≤1 ms/s CPU, input p95) NOT RUN. | progress.md | ledger |

## Polish (not blocking)

- P1 Ctrl+O file dialog is modal on the UI thread (`src/shelf_os.rs:197`): while it is open, host-loop deadlines (store flush, timer completion) wait. Known deferral.
- P2 Each UNC/mapped-drive shelf click spawns one unbounded thread (`src/shelf_os.rs:229`); repeated clicks on a hung share stack threads. Bound to one in flight.
- P3 Release binary exposes `--probe-failure`/`--probe-panic` (`src/main.rs:47-48`); gate them like `--probe-device-loss`.
- P4 Settings monitor list does not refresh while open; no "Follow Windows" theme; glass has no real backdrop blur (deferred, keep accepted visual direction).
- P5 Text-layout cache per canvas deferred; full paints still publish UIA state (M4 gate-1 deferral).

Checked and acceptable (static): icon worker shutdown bounded at 1 s then detached (`src/icons.rs:232-251`); icon cache bounded (`CACHE_LIMIT`) and evicted on catalog change; `GetWindowTextW` on foreign windows does not send messages; shelf/settings inputs length- and control-char-validated; store keeps `.bad` copy on corrupt load.

## Missing features (product, not readiness)

Live DWM previews (M3 scope), jump lists/context menu, badges, global hotkeys, audio device switch, `--timer` channel, timer cancel/pause. Do not start any before B1-B4.

## Lowest-cost order

1. **B1** (~10 lines): give `Accessibility` an idempotent `disconnect()` (root behind `Option`), call it from `Surface::drop` before `invalidate`; `Drop` calls it too. Keep the HRESULT log, no custom runtime IDs. Gate: smoke rerun shows 0 `E_INVALIDARG` in 110 cycles + handle/USER/GDI plateau.
2. **B4**: when `--diagnostics` is absent, append errors to `%LOCALAPPDATA%\ROQA\error.log` (bounded, overwrite on start) and show one `MessageBoxW` only on startup failure. No idle cost.
3. **P3, P2**: small, no runtime cost.
4. **B3**: shelf keyboard focus + "more" entry, keyboard reorder (S8). Reuse the existing notch focus model.
5. **B2**: read `SPI_GETHIGHCONTRAST` at theme build and on `WM_SETTINGCHANGE`; map to system colours. One rebuild per change, no polling.
6. **B6**: resume reviews at the M4/M5 gates, then the M5 measurement run (needs explicit native authorization).
7. **B5**: user decision on license/distribution, then a reviewed release plan.

Expected cost (estimate, not measured): items 1-5 add no thread, timer or idle wake; B4 adds one file write per error; B2 one surface rebuild per system change. Binary growth expected in single-digit KiB per item.

## Missing proof / acceptance gates

Smoke rerun after B1; picker suite (INCONCLUSIVE ×2); catalog and packaged-hung suites NOT RUN; Narrator, RTL, IME, 100-225% text scale, mixed DPI, sleep/resume, Explorer restart, device loss NOT RUN on M5 features; no theme seen on a display; M5 performance gates and Nectar/Nusa comparison NOT RUN; expansion shadow overlap NOT ESTABLISHED.

## Workspace cleanup

- Inventory: one worktree (`main`); remote `feat/m4-hardening`, `feat/m4-visualizer` already merged into `main` (left alone: deleting remote branches is outward-facing). `target/` 4.0 GB (current builds, kept), `artifacts/` 2.3 GB (evidence, kept), `.tools/` 758 MB (kept).
- Candidate: `C:\Users\Ahmed\.t3\projects\roqa-m2b2` is a stale non-git copy (2026-11-09); its `target/` (1.6 GB) is regenerable. Its `.tools` is a **junction to the live `roqa\.tools`**, so never delete the folder recursively. A Sol-low delegate was asked to delete only `roqa-m2b2\target` after a reparse-point check; the sandbox policy refused the command before it ran. **Nothing was deleted.** The user can run it by hand after the same check.
- Preserved untouched: `.gitignore` (user), `scripts/smoke.ps1`, `progress.md` WIP, the `m5-smoke-envelope-fix` and `m5-native-sol-2` packets.
