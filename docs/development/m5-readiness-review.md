# M5 readiness implementation: independent review

Reviewer: Claude Opus 5.5 (fresh, no authorship). Date: 2026-11-11 Africa/Cairo. Rubric: clean-code-guard review mode, AGENTS.md.
Inputs: root `238c2c8`; frozen manifest `m5-readiness-implementation.sha256`, SHA-256 `BACB03F1F5A309A3A84521DD1C22A5C8DC0CD63C68347162ED8015177CFF4FC2`.
Manifest check: 21/21 entries OK at start and at end; manifest SHA unchanged; HEAD unchanged. Coverage matches `git diff 238c2c8 --stat` plus untracked
packets/`error_log.rs`; `progress.md` (WIP ledger) and user `.gitignore` excluded as stated.
Scope: static reading of the whole source diff incl. preserved smoke envelope fix. No build, test, GUI/native run, measurement, edit, commit or push.
Author claims (180 tests, fmt/clippy/release/debug PASS, 1,059,840 B, SHA `968551D6…465F7`) were NOT re-run; they are cited, not verified.

## Disposition: REVISE

One high, three medium findings. Everything else is sound or low. Fix H1, M1-M3, then re-freeze and re-verify. Static review only; not acceptance.

## Performance / tiny footprint (reviewed first), static

- No new permanent thread, timer, poll or idle wake: picker worker exists only while the dialog is up; network launch admits one worker; error log does no idle I/O.
- Per-frame UIA publish now skips label rebuilds by shelf revision, media `uia_revision`+audio level, timer text and progress (`host.rs:378-417`). Sound; benefit unmeasured.
- High-contrast re-read only on WM_SETTINGCHANGE/WM_SYSCOLORCHANGE; rebuild only if the palette differs (`host.rs:2184-2193`).
- Shelf paging does not write the store (`shelf.rs:148` compares paths). Icons follow the visible page.
- Release binary +60,416 B (6%) over 999,424 B. This exceeds the assessment estimate of single-digit KiB per item. Size only; no resident-memory, CPU or latency claim. M5 budgets NOT RUN.

## Findings

| ID | Sev | Evidence | Defect | Concrete fix |
|---|---|---|---|---|
| RR-H1 | High | `error_log.rs:13-29`, `main.rs:46-50`, `host.rs:2317`, `ipc.rs:106-113` | `initialize()` truncates `error.log` before the single-instance check. A second plain launch (e.g. double-clicking the shortcut again) wipes the running instance's log. The running instance keeps writing at its old offset, so the file gets a NUL gap. The second process then fails with `E_INVALIDARG` "already running", `STARTED` is false and argc==1, so it shows a modal "could not complete startup (0x80070057). See error.log" pointing at the log it just emptied. Before this change it exited silently. | Open/truncate the log only after `Controls::create` succeeds (e.g. call `error_log::initialize()` from `host::run` after admission). Treat the already-running error as a quiet exit, or show a specific message, never the generic startup failure. |
| RR-M1 | Med | `theme.rs:172` `ink_alpha`; `graphics.rs:1297-1304` (progress track/fill), `1333-1338` (volume track/fill), `1229` (chip `DISABLED`), `712` (shelf missing) | In high contrast every alpha > 0 becomes 1.0. Bar tracks and fills draw as the same solid ink, so progress and volume levels disappear. Disabled controls, muted state and missing shelf files look the same as enabled or present ones. The B2 fix loses information visually. | Apply `ink_alpha` to text and glyphs only. Draw tracks in `p.tile` (BTNFACE) or as an outline, and draw disabled or missing items in `GetSysColor(COLOR_GRAYTEXT)` (add it to `Palette::contrast`). Add one test asserting that track ≠ fill and disabled ≠ enabled under the contrast palette. |
| RR-M2 | Med | `host.rs:1570-1594` (toggle_pin, change_pins), `1614` (drag), `1310-1341`; `settings_window.rs:429` | The pin combo is filled only when settings opens and after a Move. If a dock right-click toggles a pin or a drag reorders one while settings is open, the combo is stale. "Move pin earlier/later" then sends a stale index, which moves the wrong pin or does nothing, and the change is persisted. | Call `sync_settings_pins(current selection)` from `change_pins` when the edit returns true, so every pin change path refreshes the combo. |
| RR-M3 | Med | `host.rs:2138-2146`; `settings_window.rs:171-205` | `refresh_monitors(...)?` runs before new surfaces are placed. A USER-handle failure in a settings checkbox aborts topology placement: the new monitor gets no dock/notch and `topology_due` is already cleared. `survive` passes the non-device error through, which ends the host. Monitors were already drained, so the window is left half rebuilt. | Do the refresh after surface placement and contain its error: log it, then close the settings window. A settings-window failure must not end the host. |
| RR-L1 | Low | `settings_window.rs:211-256` vs `position` (pins at `tops[last-1]`); `refresh_monitors` recreates controls at the end of the z-order | Tab order does not match visual order. Pins come after the monitors in tab order but are drawn above Reset/Close. Recreated monitor checkboxes tab after Close. Keyboard and Narrator flow is out of sequence. | Create the pin controls just before Reset/Close, or move them visually. Insert recreated checkboxes after `self.notch` with `SetWindowPos(hwnd, prev, …, SWP_NOMOVE\|SWP_NOSIZE\|SWP_NOACTIVATE)`. |
| RR-L2 | Low | `host.rs:981,1375,2185` | A `SystemParametersInfoW(SPI_GETHIGHCONTRAST)` failure is fatal (`?`) at create, restyle and every settings-change pass. | Log it and fall back to `theme.palette()`. |
| RR-L3 | Low | `shelf_os.rs:415-417`, `host.rs:1954` | The result is collected only on the `PICKED` thread message. If `PostThreadMessageW` fails (logged), `worker` stays `Some` and Ctrl+O and Add stay dead until restart. | In `start()`, call `self.take()` first when `worker.is_some()`. |
| RR-L4 | Low | `host.rs:1676` | `change_shelf` always calls `reconcile_focus(true)`. A drop or picker add while a media control has focus re-announces that control to Narrator. | Pass `replaced` only when the focused control is a shelf slot. |
| RR-L5 | Low | `media.rs:681-682`; `settings_window.rs:504` | The `generation()` doc comment now sits on `uia_revision()`. The row-height comment omits the new pin row. | Move or update the comments. |
| RR-L6 | Low | `error_log.rs:40`, `main.rs:4-9` | The log holds only the template prefix: startup failure records `ROQA: ` with no HRESULT, yet the dialog points users there. Privacy is preserved, but the log is close to useless. | Also record the fixed HRESULT for startup failure (a non-sensitive scalar). |

## Checked and acceptable (static)

- B1: `Surface::drop` disconnects before `notch_uia::invalidate` while the HWND is alive. `disconnect` is idempotent, `Drop` repeats it safely, and the HRESULT log is kept (not suppressed). The `get_object` borrow ends before FFI, so `borrow_mut` cannot conflict on this thread.
- P3: failure/panic probes, `PANIC_PROBE` and injected faults are `cfg(debug_assertions)`. Smoke uses a separately hashed debug binary and checks build exit codes.
- P2: CAS permit with RAII release on completion, unwind or failed spawn (the closure is dropped). Per-worker STA guard is balanced, S_FALSE counts as success, RPC_E_CHANGED_MODE skips uninit. The test is meaningful.
- P1: dialog and results stay on one worker STA. Only copied `Vec<String>` crosses threads. ERROR_CANCELLED is silent, other HRESULTs are logged. Cancellation passes an HWND checked against thread identity. No join on the host. No WM_APP collision (`+60` unique; catalog ignores it).
- B3: paging covers all 12 entries and wraps (tested). Slot count ≤3 fits `enabled: [bool;7]`. Delete and right-click on More/Add are no-ops. UIA focusability follows `enabled`; stale versions return not-available (tested).
- Smoke envelope fix preserved; parse-only evidence; shadow overlap is labelled NOT ESTABLISHED.

## Deferred gates (all NOT RUN on this candidate)

Smoke rerun with 0 `E_INVALIDARG` over 110 cycles and a handle/USER/GDI plateau. Picker with timer/store deadlines, cancel/reopen, quit while open. Hung network share. Topology change with settings open. High-contrast and system-colour change and restore. Second-instance launch UX. Narrator/keyboard/RTL/IME/text scale/mixed DPI. Catalog/packaged-hung, resume, Explorer restart, device loss. M5 CPU/memory/latency/DWM budgets and Nectar/Nusa comparison. Council/CCG milestone gates. No milestone acceptance is claimed.
