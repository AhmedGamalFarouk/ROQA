# M1 iteration 2: independent Opus recheck

- Reviewer: Claude Opus 5.5 High, in a fresh delegated recheck task. I authored none of the M1 code, fixes, tests, packet or author evidence. The author is `/root`.
- Date: 2026-11-09. This is fix/recheck round 1 of 3.
- Input: `docs/development/reviews/m1-iteration-2.manifest`, SHA-256 `3080840DA84AE480AF07B382ABD4DF42D9E2E1956706EEAACB9693E3606E0D7C`, 41 entries.
- Binary under test: `target/release/roqa.exe`, SHA-256 `4C5BC51B0EE71720A157997C6999235A5EDAD8F5D7787595B661BD10D2D4236B`, 176,640 bytes.
- I did not read the iteration-2 Sol report before forming these findings.
- **Disposition: ACCEPT** for this manifest.
  - M1-OPUS-MAJ-1 is resolved.
  - All other prior Opus minors are resolved, partially resolved within the agreed deferral, or documented.
  - The three new findings are minor and do not block.

## Input verification

**Manifest.** The manifest file hash matches. I re-hashed all 41 entries after stripping CRLF, once at the start and once at the end of the review: 41/41 matched both times. No frozen input changed while I worked.

**Binary.** The binary hash matches `artifacts/m1-candidate-2/smoke.json`, the packet, and both of my own smoke and idle runs.

**Source-to-binary check.** I ran a clean `cargo build --locked --release` with the project-local toolchain into a scratch `--target-dir`. No GUI was running at the time.

- The output has the same size.
- 30 bytes differ, and all of them are metadata:
  - the PE TimeDateStamp (offsets 265–266);
  - the debug-directory timestamps and RSDS GUID (around 147445–147848);
  - path separators in an embedded dependency panic path (around 133971–134008). This difference comes from my shell setting `CARGO_HOME` with forward slashes.
- Code and data are otherwise identical, so the frozen source reproduces the frozen binary.

**Static checks.** I did not rerun fmt, clippy or the unit tests. The author ran them, and the fresh Sol owns the required build/check and the changed-code clean-code-guard pass.

**Writes and cleanup.**

- I wrote only this report and git-ignored evidence under `artifacts/m1-opus-recheck-2/`.
- `artifacts/m1-candidate-2/` and `artifacts/m1/` were not touched.
- No ROQA process is running at the end.
- Every host I started exited through `--quit` or a last-surface close with code 0. The two failure probes exited with 1, as designed.
- I stopped no unrelated app; the Nectar Dock and Nusa notch processes were left running. I changed no system scale or setting.

## Prior finding resolution

| ID | State | Basis |
| --- | --- | --- |
| M1-OPUS-MAJ-1 | **Resolved** | Source, API semantics and runtime; see below |
| M1-OPUS-MIN-1 | **Resolved** (rebuild-on-real-change deferral accepted) | Guard verified by the frozen smoke and a 200-broadcast diagnostic |
| M1-OPUS-MIN-2 | **Resolved** | Frozen harness classified real interference as INCONCLUSIVE twice, then passed |
| M1-OPUS-MIN-3 | **Resolved / partially** (non-blocking) | Interior fill and exact exit 1 are asserted; the stderr line is not |
| M1-OPUS-MIN-4 | **Resolved** | `measure-idle.ps1 -EvidenceDirectory` used successfully |
| M1-OPUS-MIN-5 | **Resolved (documented)** | Packet states fullscreen overlap until M3 |

### M1-OPUS-MAJ-1: resolved

**Code**

- `GetDpiForSystem` no longer appears anywhere in `src`, `tests` or `scripts`, confirmed by grep.
- `src/native_window.rs:147-168` (`primary_dpi`):
  - creates a hidden, never-shown 1×1 `WS_POPUP` window at the primary work-area origin, using ROQA's own class;
  - calls `GetDpiForWindow` on it;
  - treats a result of 0 as an explicit error;
  - destroys the window through `NativeWindow::drop` on the same thread, before returning.
- The embedded manifest declares `PerMonitorV2`. I found the string in the binary. No code changes the thread's DPI awareness, so the probe window is PMv2. For a PMv2 window, `GetDpiForWindow` returns the DPI of the monitor the window is on. That is the current effective DPI of the primary monitor, not the sign-in system DPI.
- `src/host.rs:101-108`: startup uses `primary_dpi(work)`.
- `src/host.rs:161-179`: when `WM_SETTINGCHANGE` or `WM_DISPLAYCHANGE` arrives (`wParam` 0), the code requeries the DPI. `WM_DPICHANGED` keeps using its `wParam` DPI.
- The author's reason for not using `GetDpiForMonitor` follows Microsoft's documented caution for per-monitor-aware calling threads. It is a valid substitute for the fix I proposed in round 1.

**Ownership and side effects** of the probe window:

- It is a `WS_EX_TOOLWINDOW` popup and is never shown, so there is no taskbar entry, no activation and no composition.
- It is created and destroyed synchronously on the UI thread.
- If any broadcast reaches it during that short lifetime, the only effect is a posted `REPOSITION`, which the guard handles. That is harmless.
- If creation fails during reconciliation, the host exits with an error rather than mis-sizing. This is the stated "explicit failure" policy, and I accept it for M1.

**Runtime.** The frozen smoke, run 5 below, passed on the identical binary:

- synthetic `WM_DPICHANGED(144)` gives notch width 312;
- a `WM_SETTINGCHANGE` then reconciles both dimensions of both surfaces to the DPI from an independent PMv2 HWND on another thread (96);
- `WM_DISPLAYCHANGE` holds that result.

**Limits that remain open**

- On this machine, the sign-in DPI and the current DPI are both 96. The frozen smoke therefore cannot tell this fix apart from the old `GetDpiForSystem` behavior at runtime.
- The fix rests on source and API review plus correct message wiring, not on a physical scale change. No real scale change, change of primary monitor, mixed DPI or topology change was performed, by me or by the author. These remain open manual or user-authorized checks, as the packet states.

### M1-OPUS-MIN-1: resolved (deferral accepted)

`src/host.rs:168-170` returns early when the DPI and work area are unchanged.

My diagnostic (`artifacts/m1-opus-recheck-2/diag/recon-diag.txt`) sent 200 unchanged broadcasts, 100 `WM_SETTINGCHANGE` to the notch and 100 `WM_DISPLAYCHANGE` to the dock:

- both surface HWNDs stayed the same;
- no "ROQA DPI Probe" window remained;
- private bytes, handles, USER and GDI were all identical before and after (8,105,984 B / 218 / 7 / 0).

So the probe window is created and destroyed per reconciliation without leaking.

Thirty full DPI→settings rebuild cycles grew handles by 3 and private bytes by 172 KiB, with USER and GDI flat. That is within the plan's tolerances.

A real change still destroys and recreates both surfaces, with no coalescing. In round 1 I accepted deferring that to M2, so it is not reopened. M2's UIA work will need stable HWNDs.

### M1-OPUS-MIN-2: resolved

**Code.** `scripts/smoke.ps1:67-89`:

- reads the cursor back 10 times at 20 ms intervals before each capture, and once more after it;
- retries at most 15 times, and only for observed interference;
- fails a stable wrong-color capture immediately;
- reports exhausted interference or a foreground refusal as `INTERFERENCE` → `INCONCLUSIVE`, which throws and cannot pass.

**Runtime.** The user was moving the mouse at the start of my slot; my sampling showed the cursor travelling across the screen.

- Runs 1 and 2 correctly reported `INCONCLUSIVE` at `notch-idle.png`. They did not report FAIL, and no ROQA defect was involved.
- Once the cursor was idle, run 5 passed with every capture stable on attempt 1.

I did not change the harness to get that result. My only intervention was to start each run, in a fresh process, after the cursor had been idle for 3 seconds.

### M1-OPUS-MIN-3: resolved (partially)

What is now asserted:

- `NativeProbe.CheckInteriorPixels` checks every interior pixel except the four 18-px corner squares and the 1-px border.
- `smoke.ps1:215` requires the exit code to equal exactly 1.
- The post-exit window check is now honestly labelled as covering orphans, with OS cleanup also applying.

What is not: the expected stderr line from the probes. This is acceptable and non-blocking.

My own full-image analysis of the six run-5 PNGs:

| Surface | Size | Dominant color, idle and leave | Dominant color, hover | Share of image |
| --- | --- | --- | --- | --- |
| Notch | 208×36 | `22,27,34` | `42,51,62` | 95.7% |
| Dock | 300×64 | `22,27,34` | `42,51,62` | 98.3% |

- Corner (0,0) shows the background, which confirms the rounded region clips the content.
- The mid-point of the left edge has the fill color.
- I inspected `notch-hover.png` and `dock-idle.png` visually: solid rounded pills, with no artifacts.

### M1-OPUS-MIN-4: resolved

`measure-idle.ps1 -SkipBuild -EvidenceDirectory artifacts/m1-opus-recheck-2/idle` ran correctly and did not touch the frozen evidence.

### M1-OPUS-MIN-5: resolved (documented)

The packet's finding table and boundaries state that the topmost surfaces overlap fullscreen apps, and that fullscreen suppression is M3 work.

## Independent runtime results (frozen harness, unmodified)

Machine: Windows 11 Pro 26200, Ryzen 5 PRO 4650G with Radeon iGPU, a single 1920×1080 monitor at 100%. Everything is under `artifacts/m1-opus-recheck-2/`.

| Run | Result | Cause |
| --- | --- | --- |
| run1 | INCONCLUSIVE at notch-idle | Real user mouse movement; drift sampled immediately afterwards |
| run2 | INCONCLUSIVE at notch-idle | Same |
| run3 | INCONCLUSIVE, foreground refused | **Invalid: my retry reused the PowerShell host** (R2-MIN-2) |
| run4 | "FAIL", StartAnchor failed | **Invalid: same reuse** (R2-MIN-2). Not a ROQA failure |
| **run5** | **PASS, 305 assertions** | Fresh `powershell.exe` started after 3 s of cursor stillness; all six captures stable on attempt 1 |

Run 5 covers the following:

- **Focus:** the anchor stays foreground at launch and after both hovers.
- **Geometry:** notch 208×36 and dock 300×64, centered and inside the work area, which is unchanged at the end.
- **DPI:**
  - synthetic DPI 144 gives notch width 312;
  - `WM_SETTINGCHANGE` and `WM_DISPLAYCHANGE` reconcile both dimensions of both surfaces to the independent PMv2 reference DPI;
  - an unchanged settings burst preserves both HWNDs.
- **Lifecycle:** independent hide/show of each surface; CLI quit exits with 0; native last-close exits with 0; both failure probes exit with exactly 1.
- **Resources:** warm and all five blocks are identical at 8,056,832 private bytes / 239 handles / 0 GDI / 7 USER. That is lower than the author's 11,055,104 / 245, but flat in the same way. This shows a plateau for this workload, not general freedom from leaks.
- `runtime.log` is empty.

**Idle** (`idle/idle.json`), same binary:

- 30.004 s sample, CPU delta 0 ms at counter resolution;
- private bytes 8,028,160 → 8,220,672; handles 218 → 221.

This is an observation only. DWM, GPU and power are not attributed, and there is no M5 or competitor claim.

## New findings (all minor, non-blocking)

### M1-OPUS-R2-MIN-1: `WM_DPICHANGED` wParam is trusted as the primary DPI

**Evidence**

`src/host.rs:163-167` uses the `wParam` of `WM_DPICHANGED` without checking it against `primary_dpi`. That `wParam` is the DPI of whichever monitor the receiving surface currently sits on.

**Consequence**

If a topology change leaves an old surface on a non-primary monitor with a different DPI, and its `WM_DPICHANGED` arrives after the `WM_DISPLAYCHANGE` reconciliation, ROQA recreates both surfaces on the new primary at the other monitor's DPI. They stay that way until the next broadcast.

**Mechanism demonstrated** (`diag/recon-diag.txt`):

- `WM_DISPLAYCHANGE` followed by a synthetic `WM_DPICHANGED(120)` leaves the surfaces at 260×45 and 375×80 on a 96-DPI primary;
- the next settings broadcast restores 208×36 and 300×64.

I have no evidence that Windows actually delivers messages in this order. Multi-monitor and topology work is M2+/M3, so this does not block.

**Suggested fix:** treat `WM_DPICHANGED` only as a trigger and always call `primary_dpi`, or cross-check the two. Keep a synthetic-DPI test path some other way.

### M1-OPUS-R2-MIN-2: smoke fixture is not re-entrant within one PowerShell host

**Evidence**

`tests/NativeProbe.cs:27-30` and `:77-101`: `AnchorReady`, `anchor` and `anchorFailure` are static. Because `Add-Type` reuses the already-loaded type, a second `smoke.ps1` invocation in the same PowerShell process breaks:

- `WaitOne` returns at once with the previous, destroyed anchor HWND;
- the new thread's `RegisterClass("ROQA.SmokeAnchor")` fails.

**Consequence**

The result is a spurious foreground INCONCLUSIVE (run3) or a status labelled `FAIL` (run4). The `FAIL` label wrongly attributes a fixture defect to the product.

**Workaround and fix**

- Workaround: run each smoke in a fresh process. Neither the author's documented commands nor my valid run were affected.
- Fix: reset the static state per start, tolerate an existing class, or classify fixture failures as INCONCLUSIVE.

### M1-OPUS-R2-MIN-3: unit-level coverage of the DPI choice is still absent (test gap)

**Evidence**

The three unit tests still cover only `placement()`. Nothing automated would catch a regression back to `GetDpiForSystem` on a machine whose scale has not changed since sign-in, which is every CI-like environment.

**Consequence and fix**

This is accepted as an inherent limit for M1, because the source review is the proof. It should be recorded so the physical scale-change check is not forgotten. One option is a short manual checklist item, to be run when the user authorizes it.

## Informal clean-code note

Sol owns the required guard. Informally, I found the change small and in keeping with the existing structure:

- the probe reuses `NativeWindow::create_hidden`;
- the guard is three lines;
- there are no new abstractions.

The renaming of `create` into `create`/`create_hidden` is reasonable.

## Evidence validity and limits

- Single machine, single monitor, 100% scale. The sign-in DPI equals the current DPI, so MAJ-1's runtime proof cannot distinguish the old and new DPI sources. The resolution rests on source and API review plus message wiring.
- Not performed: physical scale change, change of primary monitor, mixed DPI, topology change, lock/unlock, device loss, Explorer restart, or fullscreen and third-party edge-overlay coexistence. The existing Nectar/Nusa overlays kept running. The harness temporarily moved only ROQA's own windows for captures and restored them.
- The diagnostic in `diag/recon-diag.ps1` is a reviewer script that is separate from the frozen harness. Acceptance relies on the frozen run5 PASS, not on the diagnostic.
- Idle is a single 30 s endpoint sample.
- M2 scope remains separate: UIA, keyboard, text scaling, RTL/IME and reduced motion.

## Disposition

**ACCEPT** on manifest `3080840DA84AE480AF07B382ABD4DF42D9E2E1956706EEAACB9693E3606E0D7C`. M1 acceptance still requires a same-manifest Sol ACCEPT, which includes its changed-code clean-code-guard pass, before M2 begins.
