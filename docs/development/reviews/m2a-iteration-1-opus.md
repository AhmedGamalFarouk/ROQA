# M2a iteration 1 — independent Opus 5.5 High review

- Reviewer: Opus 5.5 High, fresh delegated review task. I authored none of the M2a source, tests, fixes, scripts or docs. The author is `/root`.
- Scope: the **M2a foundation only**: the real window catalog, WinEvent intake, bounded worker, model/generation handling, shutdown, and the M1 shell regression. This is **not** acceptance of full M2. It waives no M2b, cumulative M2 or stack go/no-go gate.
- Input manifest: `docs/development/reviews/m2a-iteration-1.manifest`, SHA-256 `C493F3615CBE4CEE643E3264D67C50961B09E44CD0C48A96E418A0A5BDAFBBAC`, **61 entries**.
- I did not read any other M2a review before writing these findings. I edited no frozen input, ran no install or commit, and changed no system setting. I wrote only this report and ignored evidence under `artifacts/m2a-opus-review/`.

## Disposition: **ACCEPT** (M2a foundation scope)

There are no blocker or major findings. Four minor findings follow. MIN-1 carries an explicit condition: it must be fixed before M2b introduces any modal loop. Required checks passed when I reran them. The frozen catalog and shell harnesses passed in my own independent runs.

## Input integrity

- I hashed all 61 manifest entries at the start and again at the end of the review. **61/61 matched both times.** The manifest file hash matched the value above.
- The historical M1 manifests and reports are included as hashed baseline documents only. Their old source hashes are not current integrity requirements, and I did not treat them as such.
- `target/release/roqa.exe` SHA-256 is `78A4C04E562977EE53482E11FA30ACF927AA828184BFD69641FFAA549665224F` at start and end. It matches the author's records and every runtime evidence file I produced.
- The snapshot `artifacts/review-snapshots/m2a-iteration-1.zip` has SHA-256 `AD637B9DCD402A09AAF7E7420003DDFDC4C82CD1CCF40C55131E02A5F08643D0` at start and end. It was preserved and not modified.
- `artifacts/m2a-candidate-1/` was not written.

## Independent runtime results (frozen scripts, unchanged)

The runs below were sequential. Each top-level script ran in a fresh `powershell.exe -NoProfile` host. Before the first launch I confirmed no ROQA process existed. After each run, no ROQA process or fixture remained.

| Run | Result | Key observations |
| --- | --- | --- |
| `./scripts/catalog-smoke.ps1 -SkipBuild -EvidenceDirectory artifacts/m2a-opus-review/catalog` (fresh host) | **PASS, 61 checks**, exit 0 | 501 title changes: **5 worker batches, 0 full scans, 5 delta batches, 7 HWND inspections**, 342.9 ms to observed convergence. Lifecycle samples (warm + 3 blocks) identical at **11,141,120 private bytes / 250 handles / 15 USER / 0 GDI**. Quit during the continuing title storm took 199.6 ms. Final diagnostics: `stopped:true`, `hooks_live:0`, `full_scans:2` (startup + timed repair), `delta_batches:50`, `reconciliations:1`, `discarded:0`. `runtime.log` is empty. |
| `./scripts/smoke.ps1 -SkipBuild -EvidenceDirectory artifacts/m2a-opus-review/shell` (fresh host, run 1) | **PASS, 305 checks** | Every pixel capture was stable on attempt 1. The known-foreground baseline was established, launch and hover did not steal focus, and focus-independent quit worked. 100-cycle resource blocks were flat at 11,161,600 bytes / 253 handles / 15 USER / 0 GDI. |
| Same script, same PowerShell host, `.../shell-reentry` (run 2) | **PASS, 305 checks** | Again stable on attempt 1, flat at 11,309,056 / 257 / 15 / 0. This is the static-fixture re-entry case that was INVALID in M1. |
| `./scripts/measure-idle.ps1 -SkipBuild -EvidenceDirectory artifacts/m2a-opus-review/idle` (optional) | Completed | 30.015 s. The CPU delta was **0 ms at endpoint-counter resolution** (15.625 ms quantum). Private bytes 11,079,680 → 11,280,384, handles 231 → 234. One short observation, not a benchmark or M5 verdict. |

I also reran `scripts/check.ps1` unchanged with `CARGO_TARGET_DIR` pointed at a scratch directory, so the frozen runtime binary was not rebuilt or replaced. Exit code was **0**: fmt check, all-target warnings-denied Clippy, and **7/7 unit tests** passed. stdout and stderr are captured in `artifacts/m2a-opus-review/checks/`. I did not attempt to reproduce the release binary byte-for-byte.

No external input interference occurred, so no run was INCONCLUSIVE and none needed a retry.

**Screen captures inspected** (`shell/` and `shell-reentry/`): the notch and dock idle images are uniform dark rounded capsules. The hover images are visibly lighter uniform capsules. Each leave image is byte-identical to its idle image in both runs. The four corner squares show the desktop background, which the harness correctly excludes with its 18-px corner rule. The images are byte-identical between the two runs.

### Reviewer diagnostics (not acceptance runs; separate scripts under ignored `artifacts/m2a-opus-review/`)

- `diag-passive.ps1`: ROQA with `--diagnostics` on the real desktop for 60 s.
  - The startup full scan inspected **229 raw top-level HWNDs**, with 3 eligible windows.
  - Over 60 s there were 4 event wakes, 4 delta batches and 2 full scans (both from the 30 s timer).
  - CPU was 31.25 ms in this diagnostics mode, which includes file writes. Host exit 0.
  - The raw-HWND cap of 4096 is about 18× this machine's count.
- `diag-child-storm.ps1`: a real top-level fixture with 600 `STATIC` child controls.
  - **Creating** the children produced 1123 wakes, but only **8 delta batches, 609 cheap inspections and 0 full scans**.
  - 600 distinct child renames, then 1622 sustained child renames, produced **0 wakes**.
  - Destroying the fixture took 3 delta batches and 0 full scans. Host exit 0.
  - Child-control churn does not cause ring overflow or full-scan amplification in this workload.
- `run-shell-twice.ps1` is only the runner for the same-host shell pair above. It calls `smoke.ps1` unchanged.

## Findings

### M2A-OPUS-MIN-1 (minor; must be fixed before M2b adds any modal loop): the DIRTY wake latch does not self-heal

- **Where:** `src/window_catalog.rs:65-70` sets `NOTIFIED` and posts `DIRTY` with `PostThreadMessageW`. Only the `DIRTY` handler at `:212-213` clears it. The repair timer at `:233-238` does not reset `NOTIFIED` and does not drain `HANDLES`/`OVERFLOW`.
- **Failure mode:** Microsoft's `PostThreadMessage` documentation says thread messages are lost when the recipient thread is in a modal loop (`MessageBox`, menu tracking, move/size, `DoDragDrop`, and COM STA modal waits that dispatch). If one `DIRTY` is lost, `NOTIFIED` stays `true` forever. No callback posts again, the ring fills and sets `OVERFLOW`, and nothing ever reads it. Targeted updates are then **permanently disabled** for the process lifetime and the catalog silently falls back to 30 s polling.
- A lost `COMPLETE` is recovered by the timer's `collect()`, as the comment at `:166` claims, but only after a stall of up to 30 s. During that stall `in_flight` blocks new requests.
- **Current impact:** none observed. The surfaces are `WS_POPUP` with no caption, menu, `MessageBox` or COM-pumping wait (`src/native_window.rs:64-80`). M2b's keyboard, UIA, activation and possible context-menu work makes this reachable.
- **Minimal fix:** move the DIRTY drain into a helper, and also call it from the repair timer: reset `NOTIFIED`, swap `HANDLE_COUNT`/`OVERFLOW` and drain the slots. Better, post both wakes to a message-only HWND owned by the UI thread so modal loops dispatch them. Add a unit-level test of the latch reset.

### M2A-OPUS-MIN-2 (minor): incomplete snapshots get no follow-up and no backoff

- **Where:** `src/window_catalog.rs:283-294` and `src/window_source.rs:76-81, 203-217, 226-251`.
- **What happens:** when a snapshot is incomplete (250 ms interval, 4096 raw / 512 eligible caps, cancellation), the source sets `needs_full`, but the UI schedules nothing unless a new event arrives. The model keeps the last complete records, or stays empty if the startup scan was incomplete, for up to 30 s.
- If incompleteness is persistent (more than 4096 top-level HWNDs, more than 512 eligible, or a scan that is always slow), every later event triggers another doomed full scan. They run back to back, limited only by the 50 ms settle plus up to 250 ms of scanning. Nothing is ever published, targeted updates are disabled, and there is no backoff.
- **Evidence:** static only. On this machine the raw count is about 229–280, far below the caps.
- **Minimal fix:** on an incomplete result, schedule one bounded retry. After repeated incomplete full scans, back off to the repair timer only and record a counter. Consider publishing additive, provisional records from a truncated scan, never removals.

### M2A-OPUS-MIN-3 (minor, diagnostics mode only): a failed diagnostics write kills the host

- **Where:** `src/window_catalog.rs:286`, which calls `self.write_diagnostics(false)?` inside `collect`, and `:350-358`.
- **What happens:** every published generation writes and renames a file synchronously on the UI thread. Any transient failure becomes a fatal error and ends ROQA with exit 1. A typical cause is a sharing violation on `MoveFileEx` while antivirus or the search indexer briefly holds the target. The harness reader opens with `ReadWrite|Delete` sharing, but third-party scanners do not. In an evidence run this shows up as a spurious FAIL. It is not a product failure, because normal launches write nothing.
- **Minimal fix:** treat failures of intermediate diagnostics writes as non-fatal, with a bounded retry and an error counter. Keep the final `stopped` write and the start-up path check strict.

### M2A-OPUS-MIN-4 (minor): native-source logic has no automated coverage below the harness

- **What is covered:** the 7 unit tests cover geometry, the model and the scheduler.
- **What is not:** `SourceState::reconcile` (delta insert/remove, `needs_full` stickiness, the `MAX_WINDOWS` boundary, foreground re-flagging) and the ring-overflow → `full` path in `Catalog::message` have no tests. Overflow, lost-event and time-cap behavior is therefore unverified. The author says so honestly.
- The catalog smoke's reconciliation check (`scripts/catalog-smoke.ps1:105`) proves that the 30 s timer fired. It does not prove that the timer repairs a missed change.
- **Minimal fix (before or within M2b):** inject the per-HWND inspector (or a `fn(HWND) -> Option<WindowRecord>` seam) so delta, cap and incomplete paths can be unit-tested. Add one test that forces `OVERFLOW` and asserts `full` is requested.

### Observations (no finding ID; documented limits I confirmed)

- **Eligibility changes without a show or hide** converge only at the 30 s repair. These include `WS_EX_TOOLWINDOW`/`WS_EX_APPWINDOW` toggles, owner changes and reparenting to top level (`EVENT_OBJECT_PARENTCHANGE` is not subscribed). This is acceptable for a foundation. M2b should decide whether the dock needs faster convergence.
- **Shutdown joins the worker without a deadline** (`src/window_catalog.rs:314`). Every call in the worker is non-message: `GetWindowTextW` reads foreign captions without `WM_GETTEXT`, plus `IsWindowVisible`, `GetWindowLongPtrW`, `DwmGetWindowAttribute` and limited-rights process query. A hung application therefore cannot block it. A blocked DWM or kernel call could. The candidate states this as a cooperative bound, and the M3 hung-source gate stays open.
- **Grouping by `ApplicationFrameHost.exe` and shared hosts is provisional**, as documented. The M2b stable packaged-identity work remains required.
- **`verify.ps1` has a 60 s per-step timeout.** On a cold build this could produce a false FAIL, never a false PASS. This does not block. I consider `M1-SOL-R2-MIN-1` (capture native results with explicit exit codes) addressed by `verify.ps1`'s captured streams and recorded exit codes.

## Prior carry-forward items

- **`M1-OPUS-R2-MIN-2` (static fixture reuse in the same PowerShell host): resolved within this review's evidence.** `StartAnchor` now resets readiness and state, rejects a live prior fixture, and unregisters its class in the thread's `finally`. `StopAnchor` joins with a timeout (`tests/NativeProbe.cs:81-129`). Two complete shell runs in one PowerShell host both passed 305 checks on first-attempt captures.
- **Foreground-baseline fallback** (`scripts/smoke.ps1:97-110, 58-66`): acceptable.
  - It still fails if foreground moves into a ROQA-owned window.
  - Foreground moving to anything else is INCONCLUSIVE, not a pass.
  - It requires a stable, nonzero, unchanged pre-launch foreground.
  - It no longer requires Windows to grant the background test foreground rights.
- **`M1-OPUS-R2-MIN-1`** (DPI-message authority/topology) and **`M1-OPUS-R2-MIN-3`** (physical DPI change) remain open M1/M3 follow-ups. Nothing in M2a changes them. No real scale change was performed.

## Architecture verification against the packet (static and runtime)

- **Hooks.**
  - There are six `WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS` registrations on the UI thread (`src/window_catalog.rs:177-201`).
  - The callback filters to `OBJID_WINDOW`/`CHILDID_SELF` for object events and does only scalar atomic work. It contains nothing that panics: indexing is guarded by `slot < RING_CAPACITY`.
  - Out-of-context callbacks run on the registering thread, so the ring and the `DIRTY` drain never race.
  - Unhook happens on the registering thread after `TARGET` is cleared, so late events are ignored.
- **Bounds.**
  - Ring: 256.
  - Pending: 512, with overflow escalating to a full scan.
  - Batching: one 50 ms fixed window, shared by follow-ups.
  - One request in flight, with capacity-one channels. `try_send` cannot hit `Full` under the `in_flight` invariant.
  - Enumeration caps: 4096 raw and 512 eligible, plus a 250 ms check between calls.
  - The message drain is bounded at 128 messages or 4 ms, and `MWMO_INPUTAVAILABLE` re-wakes the loop.
- **Model.** Stale and duplicate generations are rejected. An incomplete snapshot keeps the last complete records and marks uncertainty. Records are keyed by HWND plus PID, so a reused HWND cannot inherit an old PID.
- **Foreign safety.**
  - Own-process windows are excluded before `GetWindowTextW`.
  - No `SendMessage` goes to foreign windows.
  - Process access uses limited rights only. `OwnedHandle` closes the handle on every path.
  - The PID and `IsWindow` are re-checked after the identity query.
- **Shutdown.** It clears the target, unhooks, kills timers, sets cancel, drops both channel ends (releasing a blocked send), joins the worker, and only then records `stopped`. `host::run` evaluates `catalog.shutdown()` even when the event loop fails (`result.and(...)` is eager). A second shutdown from `Drop` is idempotent. Runtime evidence: `stopped:true, hooks_live:0`, exit 0 during a continuing storm.
- **Diagnostics privacy.** Output contains only numeric fields and title lengths. No titles or paths are written, as claimed.

## Gates NOT verified or accepted here (full M2 remains open)

- Not tested: elevated, packaged, UIAccess or secure-desktop windows; a genuinely hung foreign app; injected dropped WinEvents, failed `PostThreadMessage`, forced ring overflow, or native-call failure injection.
- Not tested: real display-scale or topology changes.
- No icons, stable packaged identity, activation, keyboard, UIA or Narrator, Arabic/RTL **rendering** (the Arabic check is UTF-16 title-length metadata only), system text scaling, or IME.
- No cumulative M2 stack go/no-go.
- All of these remain required for M2b and full M2. M3 cannot start before full M2 passes both reviewers on one identical manifest.
- The resource and idle numbers above are single-workload observations on this machine (Ryzen 5 PRO 4650G, Radeon iGPU, 1920×1080 @ 144 Hz, Windows 11 26200). They are not leak-freedom proofs, M5 budget results or competitor comparisons.

## Limits of this review

- **No clean-code-guard pass.** I did not run the skill; that pass is assigned to the fresh Sol reviewer. My informal reading found no clean-code issue that rises above the minors listed.
- **Unverified API behavior.** I did not re-fetch Microsoft documentation during this review. The `PostThreadMessage` modal-loop loss and the claim that `GetWindowTextW` sends no message across processes are long-standing documented behaviors that I relied on from knowledge.
- **No byte-reproducible release build.**
