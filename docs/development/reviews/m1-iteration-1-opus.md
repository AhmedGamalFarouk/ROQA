# M1 iteration 1: independent Opus review

- Reviewer: Claude Opus 5.5 High, a fresh delegated review task. I did not author any M1 candidate file. The author is `/root`.
- Date: 2026-11-09.
- Input: `docs/development/reviews/m1-iteration-1.manifest`, SHA-256 `76B1A9EC66BF2033B8DD095B51993A4120B398F8ECF229A25FE87914536EFA8B`.
- Binary under test: `target/release/roqa.exe`, SHA-256 `9411F5548BF9A5E6370F87B0AB4AD090142B3C33948EDA61842BA5C765AE6CA7`.
- I did not read the Sol M1 review before forming these findings.
- **Disposition: REVISE**, scoped to the manifest above. One major finding (M1-OPUS-MAJ-1) is open. All other findings are minor and do not block.

## Input verification

- The manifest hash matches. All 36 entries were re-hashed after stripping CRLF, at the start and again at the end of the review: 36/36 match, so no frozen input changed.
- The release binary hash matches `artifacts/m1/smoke.json`, `artifacts/m1/idle.json` and the packet.
- **Source-to-binary check.** I ran a clean `cargo build --locked --release` into a separate scratch `--target-dir`. The result has the same size (174,080 bytes). Only 24 bytes differ, and all of them are link metadata:
  - the PE header TimeDateStamp (file offsets 265–266);
  - three debug-directory timestamps (145333–145390);
  - the RSDS PDB GUID/age (145785–145800).
  
  Code and data are identical, so the frozen source reproduces the frozen binary. This is not a byte-identical (`/Brepro`) build.
- **Fresh static checks** in the scratch target dir: `cargo fmt --check` exit 0; `cargo clippy --locked --all-targets -- -D warnings` exit 0 (fresh, not cached); `cargo test --locked` 3/3 pass.
- I wrote only this report and ignored evidence under `artifacts/m1-opus-review*`. `artifacts/m1/` was not touched. No ROQA process is left running, and every host I started exited with code 0 through `--quit`. No unrelated app was stopped and no system setting was changed.

## Independent runtime results

Machine: Windows 11 Pro 26200, Ryzen 5 PRO 4650G with Radeon iGPU, 1920×1080 at 144 Hz, 100% scale. `GetDpiForSystem` = primary effective DPI = 96.

**Environment caveat:** the user was physically moving the mouse during my live slot. A cursor-stability probe set the cursor to (960,548) and read it back drifting within 150–600 ms, for example `960,546 918,561 831,643 740,722`.

| Run | Harness | Result | Cause |
| --- | --- | --- | --- |
| `artifacts/m1-opus-review` | frozen `scripts/smoke.ps1 -SkipBuild` | FAIL at check 2, before ROQA launched | `SetForegroundWindow` foreground-lock refusal; the anchor was not foreground. Harness precondition, not ROQA |
| `…-run2` | frozen smoke | FAIL at check 11: notch hover observed `22,27,34` | Concurrent user mouse movement (see diag) |
| `…-run3` | frozen smoke | FAIL at check 21: dock hover; notch idle/hover/leave passed | Same |
| `…-diag/diag-hover.txt` | reviewer diagnostic | Hover toggles `42,51,62` ↔ `22,27,34` exactly as the real cursor (read back) entered and left the isolated surface | Confirms ROQA hover works; the failures were input interference |
| `…-guarded` | reviewer copy of the frozen smoke with only these changes: each pixel capture is retried until the cursor read back equals the commanded point before and after capture (≤15 tries), and the probe exit-code check is tightened to `-eq 1` | **PASS, 288 checks** (282 frozen-equivalent + 6 cursor-stability) | Notch hover needed 7 tries and dock idle 3; all others 1 |

The guarded harness, diagnostics and logs are in `artifacts/m1-opus-review-guarded/` (scripts in `harness/`). The `-u` diff from the frozen smoke is limited to `Assert-Pixels`, the cursor-target assignments, the `$ProjectRoot` parameter and the probe exit-code check.

What the guarded run (same binary) independently establishes:

- **Focus:** a dedicated anchor stays foreground after launch and after each hover. Nonactivation holds (`WS_EX_NOACTIVATE`, `MA_NOACTIVATE`, `SW_SHOWNOACTIVATE`).
- **Geometry:** notch 208×36 and dock 300×64 at 96 DPI, centered and inside the work area. The work area is unchanged at the end.
- **Rendered pixels:** I analyzed every pixel of the captures myself, beyond the harness's single center pixel. In all 12 PNGs (author's and mine):
  - the fill inside the rounded inset is 100% uniform;
  - the dominant color is exactly `22,27,34` for idle/leave and `42,51,62` for hover, covering 95.7% of the notch and 98.3% of the dock;
  - the corners outside the window region show background, confirming the `SetWindowRgn` rounding clips DirectComposition content.
  
  I inspected `dock-hover.png` and the failed `run2/notch-hover.png` visually.
- **Synthetic DPI:** `WM_DPICHANGED(144)` with a valid RECT gives notch width 312. A following `WM_DISPLAYCHANGE` returns it to 208 (see MAJ-1 for why this "reconciliation" is the defect, not a pass).
- **Lifecycle:** independent hide/show of each surface; `--quit` exits within 5 s with code 0; native `WM_CLOSE` of one surface keeps the host alive; closing the last surface exits with 0.
- **Resource cycles:** 10 warm-up plus 5×20 notch destroy/create cycles with the dock alive:

  | Sample | Private bytes | Handles | GDI | USER |
  | --- | --- | --- | --- | --- |
  | warm | 11,350,016 | 244 | 0 | 7 |
  | blocks 1–4 | identical to warm | | | |
  | block 5 | 11,378,688 (+28 KiB) | 244 | 0 | 7 |

  This passes the plan's tolerances, with no monotonic growth. My reposition probe added 50 more full recreations of both surfaces: handles 225→225.
- **Failure probes:** I ran these with stderr captured and the exit code read directly.
  - `--probe-failure` → exit 1, `ROQA: Injected failure after first composed window. (0x80004005)`.
  - `--probe-panic` → exit 1. Stderr shows the panic at `src\native_window.rs:188` and then `ROQA: Window callback failed; orderly shutdown completed.` So `catch_unwind` caught the panic, `PostQuitMessage(70)` was observed by the main loop, and `run()` returned through normal drops. It did not abort; an abort would give `0xC0000409`.
  - Neither probe leaves a ROQA window behind.
- **Idle**, same method as `measure-idle.ps1`, recorded in `idle-reviewer.json`: 30.016 s; 15.625 ms CPU (one scheduler quantum, possibly from the user's mouse crossing a surface); private bytes 11,018,240 → 11,231,232; handles 225 → 228. This agrees with the author's ~11 MiB / near-zero CPU observation. It is observation only: DWM, GPU and power are not attributed, and this is not an M5 verdict.

The runtime gates M1 actually defines are reproduced on this binary, except that the frozen harness itself did not pass unmodified under concurrent user input (MIN-2). Only the DPI contract fails, as described in MAJ-1, and that failure is established from API semantics plus a mechanism probe, not from a physical scale change.

## Findings

### M1-OPUS-MAJ-1: surfaces are sized from the stale system DPI, and real DPI changes are reverted (major)

**Evidence**

- `src/native_window.rs:144-146`: `initial_dpi()` returns `GetDpiForSystem()`.
- `src/host.rs:104` uses it for the initial geometry.
- `src/host.rs:158-161`: `reposition` uses it for every `WM_DISPLAYCHANGE` and `WM_SETTINGCHANGE` (`src/native_window.rs:179-185` posts `WPARAM(0)` for those).
- For a per-monitor-v2 process, `GetDpiForSystem` is the *system* DPI, meaning the primary display's scale **at sign-in**. It does not follow later scale changes or a change of primary monitor until sign-out.

**Consequence**

- If the user changes the primary display's scale (say 100% → 150%) without signing out, `WM_DPICHANGED(144)` resizes correctly. The next `WM_SETTINGCHANGE` or `WM_DISPLAYCHANGE` then recreates both surfaces at 96 DPI: two-thirds of the size on a 150% display, with an unscaled 12-DIP margin. Windows broadcasts these messages often (work area, theme, tray settings).
- Launching ROQA after such a change is wrong from the first frame.
- The same applies when the primary display is switched to a monitor with a different DPI. That is still a primary-monitor M1 case.

**Probe** (`artifacts/m1-opus-review-guarded/reposition-probe.txt`): after synthetic `WM_DPICHANGED(144)`, the notch/dock are 312/450 wide. One `WM_SETTINGCHANGE` sent only to the dock returns both to 208/300, i.e. the `GetDpiForSystem` scale. On this machine system DPI equals the real DPI (96/96), so the wrong *size* cannot be shown here without changing a system setting, which I did not do. The reversion mechanism is demonstrated.

**Why the tests miss it**

- `scripts/smoke.ps1:119-124` asserts that "display-change reconciliation returns to actual system scale". This bakes the stale-DPI behavior into a passing check.
- The unit tests only cover `placement()` arithmetic.
- The plan requires DPI awareness and display geometry for M1 (PLAN.md M1 deliverables and "exercise `WM_DPICHANGED` geometry").

**Minimal fix**

1. Derive DPI from the primary monitor's effective DPI: `GetDpiForMonitor(MonitorFromPoint(0,0, MONITOR_DEFAULTTOPRIMARY), MDT_EFFECTIVE_DPI)`. Use it at startup and on display/settings reconciliation, and keep the `WM_DPICHANGED` wParam path.
2. Change the smoke reconciliation assertion to compare against that monitor DPI.
3. Add one check that the DPI from a prior `WM_DPICHANGED` survives an unrelated `WM_SETTINGCHANGE` when the monitor DPI agrees with it.

A physical scale change remains a manual or user-authorized check. Record it as open if it is not run.

### M1-OPUS-MIN-1: any settings broadcast destroys and recreates both surfaces, twice (minor)

`src/native_window.rs:179-186` maps every `WM_SETTINGCHANGE`/`WM_DISPLAYCHANGE` to `REPOSITION`, and `src/host.rs:165` clears and recreates *all* surfaces. Each top-level receives the broadcast, so one broadcast means two full HWND + composition rebuilds.

The probe showed one `WM_SETTINGCHANGE` to the notch alone gave both new HWNDs. Twenty-five simulated broadcasts all stayed handle-flat (no leak) but each rebuilt the shell. The costs are flicker, lost hover state, unstable HWND identity (M2 UIA will need stable elements), and a host exit if a transient create fails mid-rebuild.

**Fix:** coalesce to one pending reposition. Act only when the work area or DPI actually changed. Prefer `SetWindowPos` plus surface/region resize over recreation. Not blocking for a technical shell; worth doing alongside MAJ-1.

### M1-OPUS-MIN-2: the frozen smoke harness false-FAILs under ordinary concurrent input (minor)

**Evidence:**

- `scripts/smoke.ps1:99-105` captures after a fixed 150–200 ms delay, with no check that the cursor is still where `SetCursorPos` put it.
- `scripts/smoke.ps1:72` relies on `SetForegroundWindow` from the test process, which Windows foreground-lock rules can refuse.

Three of three unmodified runs failed on this interactive desktop for these reasons: one foreground refusal and two hover misses. These were false failures, not false passes; hover was always asserted before leave.

**Fix:** read the cursor back before and after each capture and retry a bounded number of times, as in `harness/smoke-reviewer.ps1`. Report the anchor-foreground precondition as "environment not ready" rather than a product FAIL. Optionally document that the run needs an idle mouse.

### M1-OPUS-MIN-3: several smoke assertions are weaker than their labels (minor)

**Evidence:**

- `tests/NativeProbe.cs:115` checks only the center pixel.
- `scripts/smoke.ps1:171` accepts any nonzero probe exit. An abort (`0xC0000409`) or argument error would also pass.
- `scripts/smoke.ps1:173` "Failure leaves no windows" runs after process exit, when the OS has already destroyed every HWND, so it cannot show RAII cleanup.

I verified the real behavior independently (full-image uniformity; exit 1 plus orderly stderr text for both probes), so the gate evidence exists and this does not block.

**Fix:**

- Assert interior uniformity and that corner pixels lie outside the surface color.
- Assert `ExitCode -eq 1` and the expected stderr line for each probe.
- Relabel the window check, or add an in-process observation before exit.

### M1-OPUS-MIN-4: the idle script can only overwrite frozen evidence (minor)

`scripts/measure-idle.ps1:6` hard-codes `artifacts/m1` and has no `-EvidenceDirectory` parameter, unlike `smoke.ps1`. A reviewer cannot reproduce the idle observation without mutating frozen evidence. I reproduced the method in a scratch script instead.

**Fix:** add the same `-EvidenceDirectory` parameter.

### M1-OPUS-MIN-5: known-limits omission (minor, docs)

The surfaces are `WS_EX_TOPMOST` (`src/native_window.rs:63`) on the primary monitor and stay over fullscreen video and games. Fullscreen suppression is correctly an M3 item, but `docs/development/m1-candidate-1.md` "Known limits" does not say so. Users of the M1 shell should be told.

## Graphics and lifecycle review: verified, no finding

- **COM:** an STA via `CoInitializeEx`, balanced on drop, including the `S_FALSE` case. DComp, D2D and D3D objects are created and used only on the UI thread. A hardware D3D11 BGRA device is required, with no silent WARP fallback.
- **Teardown order:** this is correct by Rust field and local drop order.
  - `Surface { canvas, window }`: the canvas drops first, running `SetRoot(None)` + `Commit`, then `DestroyWindow`.
  - `Desktop { surfaces, graphics }`: surfaces drop before the device.
  - In `run()`, locals drop as desktop → controls → class → apartment.
  - On partial init, the early return drops the half-built `Desktop`.
- **Drawing:** `IDCompositionSurface::BeginDraw` returns a D2D context. The code correctly does not call D2D `BeginDraw`, and `EndDraw` runs even when drawing fails. Content is committed before `ShowWindow`, so there is no blank first frame. `WS_EX_NOREDIRECTIONBITMAP` plus a topmost DComp target means no GDI redirection surface.
- **Callbacks:** the window procedure holds no Rust pointers. State changes happen in the owning loop, so a nested `SendMessage` during `CreateWindowExW` cannot re-enter `Desktop`. `catch_unwind` surrounds all dispatch, and the runtime probe confirms the orderly path.
- **Wait loop:** `MsgWaitForMultipleObjectsEx(…, INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE)` followed by a full `PeekMessage` drain. There is no lost-wake window and no timer or render tick, consistent with the idle CPU.
- **Hover:** `TrackMouseEvent(TME_LEAVE)` is armed per enter; leave reverts. The rounded region excludes corners from hit-testing.
- **IPC:** `Local\ROQA.<SID>.<action>` with the default token DACL. The client opens with `EVENT_MODIFY_STATE` only. The mutex blocks a second resident instance. `--quit` reports delivery separately from host exit, and the smoke checks host exit separately. The allowlisted commands carry no payload. SID and token buffers are released.
- **Device loss:** a device-removed error during paint ends the host with an explicit error rather than hanging. Recovery is correctly deferred to M3.

## Clean-code note

I did an informal clean-code pass. Sol owns the required independent clean-code-guard pass. Modules are small and focused, unsafe blocks are documented and clippy-enforced, and I found no dead abstractions. My only structural concern is MIN-1: rebuilding everything on reposition, where adjusting would be enough.

## Evidence validity and limits

- My runtime evidence is single-machine, single-monitor, 100% scale, on an interactive desktop with concurrent user input.
- The guarded harness is a reviewer derivative of the frozen smoke, not the frozen script. Its only behavioral changes are listed above.
- DPI testing is synthetic only. No physical scale change, mixed-DPI, lock/unlock, device loss or Explorer restart was tested; most of those belong to M3.
- The existing third-party notch overlay at the top edge was not stopped. Edge coexistence is not proven.
- The idle figures are a single 30 s endpoint sample.

## Recheck expectations

Resolving MAJ-1 needs:

- the code change;
- an updated smoke DPI assertion;
- a new manifest;
- a fresh guarded or fixed smoke PASS on the rebuilt binary.

The physical scale-change check stays explicitly open unless the user authorizes it. Minor findings may be deferred with a reason. For the recheck I would accept MIN-1 deferred to M2, provided MAJ-1's fix keeps the DPI correct.
