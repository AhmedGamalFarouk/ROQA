# M2b1 iteration 1: independent Opus 5.5 High initial review

- **Reviewer:** Opus 5.5 High, run as a fresh delegated review task. I authored no M2b1 input: no source, harness, fix, packet or evidence. The author is `/root`, continuing from thread `53d0a5f2-…`. I implemented nothing, spawned no agents, made no commit and did not read any M2b1 Sol output.
- **Scope:** initial review of M2b1 candidate 1, the modeless standard-control window picker on the live catalog. I held the exclusive native-smoke slot. This review does **not** accept full M2, the stack go/no-go or any M2b2 item.
- **Input manifest:** `docs/development/reviews/m2b1-iteration-1.manifest`, SHA-256 **`22923E0D0F08A38CE05B2FCA45BE09DD9FAA2A8B3708269CB8F90B4F08755118`**, **117 entries**.
- **Baseline:** the accepted M2a manifest `1E8AEE10CA8DC9C426B096B3AE122609E1BE032F639B55AEC1028A65B7BE2247`, through snapshot `artifacts/review-snapshots/m2a-iteration-2.zip` (`11D73C32…F22DE`).

## Disposition: **REVISE**

- **One major finding is open:** M2B1-OPUS-MAJ-1. On the product's own keyboard route (`--show-picker`), the picker usually does not take the foreground. The verified claim holds only because the smoke harness grants the foreground right on the product's behalf, and the picker's denial message is overwritten within seconds.
- **Everything else I tested works:**
  - pointer route
  - keyboard routes inside the picker
  - live refresh
  - activation, including a minimized target
  - quit with the picker open
  - UIA exposure
  - Arabic rendering
  - lifecycle
  - catalog and shell regressions
  - fmt, Clippy and tests
- **No blocker found.**
- **Minors (nonblocking):** MIN-1 to MIN-4 below.

## Input integrity

- **Manifest:** the manifest's own hash and all **117/117** entry hashes matched at start (`artifacts/m2b1-opus-review/manifest-start.txt`). They matched again after every native run and check (`manifest-end.txt`).
- **Frozen binaries:**
  - `target/release/roqa.exe` was `9D9F0A52…EAC3A7` at start and end.
  - `roqa-uia-probe.exe` was `595A0211…B6EC6E` at start and end.
  - Every picker, catalog and shell evidence JSON I produced records the `roqa.exe` hash.
  - I did not rebuild into `target/`.
- **Snapshot:** `artifacts/review-snapshots/m2b1-iteration-1.zip` hashed `6E34FC67217A6A86964C0EF7A4D8B06AA7D523D0E47A0A376399CE30705F7820` at start and end. All 117 manifest contents are present in it by hash, but its paths are flattened (see MIN-4).
- **Diff against M2a:** I compared the manifest files with the accepted M2a snapshot in a scratch tree outside the repo.
  - **New files:** `src/picker.rs`, `picker_font.rs`, `text_settings.rs`, `scripts/picker-smoke.ps1`, `tests/PickerProbe.cs` and `tests/support/uia_probe.rs`.
  - **Changed files:**
    - `host.rs`: `open_picker`, the per-pass `synchronize`, the dock `WM_LBUTTONUP` route and picker message/`IsDialogMessageW` routing.
    - `ipc.rs`: the `ShowPicker` event.
    - `window_catalog.rs`: a `model()` accessor only.
    - `Cargo.toml`: WinRT/ViewManagement features and the probe `[[bin]]`.
    - `app.manifest`: the Common-Controls v6 dependency.
    - `run.ps1`: `show-picker`.
    - `README.md`.
  - **Byte-identical to M2a:** `catalog-smoke.ps1`, `smoke.ps1` and `NativeProbe.cs`. The diff agrees with the packet.
- **What I wrote:** I wrote nothing to source, the manifest, `target/release` or `artifacts/m2b1-candidate-1/`. My own scripts are under `artifacts/m2b1-opus-review/scripts/`. No ROQA process existed before any run or remained after it.

## Runtime evidence (exclusive native slot; frozen scripts unmodified; all runs kept)

| Run | Result | Notes |
| --- | --- | --- |
| `picker-1` … `picker-15`: `scripts/picker-smoke.ps1 -SkipBuild`, each in a fresh `powershell -NoProfile` | **15 × PASS 217**, exit 0 | **Behavior:** Arabic UIA filter code points `[1605,1585,1581,1576,1575,1611]` (`مرحباً`); `EditFontHeight -15`; Enter activation; live rename; quit with picker open exits 0. Summary: `picker-runs.txt`.<br>**Resources:** private bytes 9.1–9.6 MB; handles 332–339; USER 25–26; GDI 17–18.<br>**Block-3 step:** every run shows +3 handles at block 3, which the lifecycle run below resolves as a one-time step. |
| `picker-16` | **INCONCLUSIVE** (2 checks) | "Windows denied initial fixture foreground". The user was active: last input was about 2 s earlier, so this is real desktop interference. |
| `investigate-1`, `investigate-2` (reviewer script) | **INCONCLUSIVE** | Same initial-foreground denial while the user was active. After this I ran every reviewer script through `scripts/run-when-idle.ps1`, which waits for ≥20 s without user input. |
| `investigate-3-invalid-shadowed-var` | **Invalid, discarded** | My script bug: the `WaitFor` parameter `$p` shadowed the picker handle. Kept for audit; no conclusion is drawn from it. |
| `investigate-4-sysmenu-invalid` | **Partly invalid** | Its Alt+Space step opened the user's **PowerToys Run** launcher, a global Alt+Space hotkey, instead of the picker's system menu. The SystemMenu and Reopen scenarios are invalid. I dismissed the launcher I had opened with one guarded Escape while it was foreground, and verified it hidden. This was a reviewer side effect on an unrelated app, disclosed here. Its other scenarios agree with run 5. |
| **`investigate-5`** (reviewer script, idle-gated) | Completed | Full results in the next table. |
| `lifecycle` (reviewer script) | **210 cycles completed**, exit 0 | **Setup:** one resident, fixture foreground, then a cycle of granted open → foreground → Escape → fixture restored.<br>**Samples:** pre-picker 8.07 MB / 253 handles / 14 USER / 0 GDI; warm (10 cycles) 8.90 MB / 307 / 24 / 15.<br>**Blocks 1–10:** 8.70–8.97 MB. Handles 307 → **310 once, between cycles 50 and 70**, then flat through cycle 210. USER 24 and GDI 16 stay flat.<br>**Cooldown (10 s):** 8.81 MB / 310 / 22 / 16.<br>**Reading:** a plateau with no growth trend. These are workload samples, not a leak-freedom proof. |
| `catalog`: `catalog-smoke.ps1 -SkipBuild` | **FAIL**, 60 checks | Failed only "Probe reached posted-message queue capacity". The producer posted 3,659,616 messages with **0 rejected**, so ROQA kept up and the workload precondition was not met. Generation 618 and 1 reconciliation still progressed. The script is unchanged from M2a, so this is a harness classification issue (an unsaturated workload reported as FAIL), not a product regression. It is not a finding, because the rules exclude new minors on unchanged content. |
| `catalog-2`, `catalog-3` | **PASS 67**, PASS 67 | **Queue traffic:** 2,859,165 posted with 822 rejected, and 2,858,716 posted with 3,537 rejected.<br>**Title storm:** 501 changes → 6 targeted batches, 0 full scans, 371–394 ms.<br>**Resources:** flat at about 8.2–8.45 MB / 247 handles / 14 USER / 0 GDI. |
| `shell`: `smoke.ps1 -SkipBuild` | **PASS 305** | M1 shell regression on the picker-capable loop. I inspected `dock-hover.png`; it shows the expected capsule. |
| `checks/`: fmt, all-target Clippy `-D warnings` and `cargo test --locked` | **All exit 0**, **11/11 tests** | Run on a scratch tree of the 117 verified files with the project-local `.tools` toolchain, offline, with `CARGO_TARGET_DIR` set to scratch. Logs are UTF-16 and exit codes are in `exit-codes.txt`. I make no byte-reproducibility claim for the frozen binary. |

### `investigate-5` (idle-gated; each scenario repeated 5×)

The machine's `ForegroundLockTimeout` is **2147483647** (read only), so Windows never grants foreground on a timeout here.

| Scenario | Result |
| --- | --- |
| Passive launch, foreground sampled at dock appearance, +0.5 s and +2 s | **5/5 kept the fixture** (plus 5/5 in run 4). |
| **Pointer route:** real `SendInput` click on the dock capsule, after hit-testing that the point is the ROQA dock | **5/5:** picker took the foreground, focus was in the filter edit, Escape returned the foreground to the fixture. |
| `--show-picker` client launched by the foreground process **without** the harness's `AllowSetForegroundWindow` (the real CLI route) | **1/5 foreground**, 4/5 denied (run 4: also 1/5). In the denied cases the status was "Click this window…" at open in 3 of 4. **At 3 s, every case showed "6 windows. Filter, choose a window, then press Enter."** |
| `--show-picker` with the harness grant | **5/5 foreground.** |
| Grant, then one injected Shift keystroke to the fixture, then signal | **0/5 foreground** (run 4: 0/5). Any input between the grant and its use revokes the grant. |
| Tab order | Filter → list → Open → Close → filter. Shift+Tab from the filter goes to Close. |
| Alt accelerators and keys | Alt+F focuses the filter. Alt+C closes and returns the fixture. Alt+O activates. Enter on Close closes. Alt+F4 closes. The resident stays alive. |
| Live refresh | A second reviewer-owned window was listed. After the *other* row was renamed, the selection stayed on the same window (run 4: also when the *selected* row was renamed). Focus stayed in the list and the picker stayed foreground. |
| Minimized target | Minimizing B → filter → Enter: **B restored and foreground**. |
| Target closed while the picker is open | The row disappeared, the picker kept the foreground and the resident survived. |
| Window-local keyboard move loop (`WM_SYSCOMMAND SC_MOVE` posted to the picker; `GUI_INMOVESIZE` confirmed) | A fixture rename was **not** published within 2 s during the loop. After Escape ended it, the rename arrived in **50 ms**. |
| Second open request while open | The picker was recreated and took the foreground. Escape returned to the fixture, but only because of z-order, since `return_to` then names ROQA's own old picker. |

### Visual check

I inspected `picker-15/arabic-picker.png`:

- The mixed Latin/Arabic title renders with joined Arabic shaping and correct RTL run order inside the LTR list row.
- The filter edit shows `مرحباً`.
- Mnemonic underlines F, O and C are visible.
- The status reads "1 windows." (a grammar nit).

The capture rectangle includes the DWM invisible border, so slivers of neighboring screen content (other apps' text) land in the evidence PNGs. This is an evidence-hygiene note on the unchanged capture helper, not a finding.

## Investigation of the author's unattributed foreground failures

**Source.** `Picker::create` (`src/picker.rs:111-120`):

1. creates the hidden dialog;
2. shows it with `SW_SHOWNOACTIVATE`;
3. calls `SetForegroundWindow` synchronously on the UI thread that consumed the event or click;
4. sets focus only if that call succeeded.

No other ROQA code activates anything:

- The dock and notch are `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` with `MA_NOACTIVATE` (`native_window.rs:68,190`).
- The startup path never activates.
- An existing picker is destroyed *before* its replacement is created (`host.rs:130-134`).

I found no product focus race in this path: there is no asynchronous gap and no competing ROQA activation. The product does **not** call `AllowSetForegroundWindow` anywhere. The `--show-picker` client (`ipc.rs:60-71`) only calls `SetEvent`.

**`picker-failed-run2`** (picker did not take the foreground during the cycle loop):

- **Mechanism.** The harness grants the right with `PickerProbe.AllowSetForegroundWindow(resident)` (`picker-smoke.ps1:44`), then launches the client. I showed that a single input event between grant and use revokes the grant: 0/5 succeeded in each of two runs.
- **Environment.** The foreground lock timeout is effectively infinite on this machine, so nothing else rescues the call.
- **User activity.** I observed the user actively using the desktop during this session: three INCONCLUSIVE initial-foreground denials, with last input about 2 s earlier.
- **Idle comparison.** On an idle desktop, about 570 granted opens across 15 frozen runs, 210 lifecycle opens, 10 further granted opens and 10 dock clicks gave **zero** picker foreground failures.
- **Conclusion.** This failure is most consistent with **user input revoking the harness grant**, which is desktop interference, not a product race. It is not proven for that specific run, because no owner was recorded. The product-side weakness it exposes is MAJ-1: the IPC route depends on a right someone else grants.

**`picker-run5`** (passive launch changed the foreground):

- **Source.** No activation mechanism exists on the startup path.
- **Runs.** 10/10 of my instrumented passive launches (sampled up to +2 s) and 15/15 frozen passes kept the fixture.
- **Conclusion.** This is most consistent with the user changing the foreground. It is not proven, because the old harness recorded no owner.

## Findings

### M2B1-OPUS-MAJ-1 (major): the `--show-picker` keyboard route does not reliably take the foreground; the harness supplies the missing grant and the denial notice is overwritten

**Evidence:**

- **Harness grant.** `picker-smoke.ps1:44` calls `AllowSetForegroundWindow(resident pid)` from the foreground test process before every `--show-picker`. The product's own client (`ipc.rs:60-71`, used by `scripts/run.ps1 show-picker` and documented in the README as the way to open the picker) makes no such hand-off.
- **Real route without the grant** (`investigate-4`, `investigate-5`): the picker reached the foreground **2/10** times. With the grant: 10/10.
- **Status overwritten.** On denial, `Picker::create` sets "Click this window…" (`picker.rs:118`). The next catalog generation then runs `refilter` → `set_status` (`picker.rs:270`), which replaces it. In every denied case the status read "N windows. Filter, choose a window, then press Enter." within 3 s, telling a keyboard user to press Enter in a window that does not have focus.
- **Overstated claim.** The packet's verified claim "explicit open creates the dialog and takes foreground" therefore holds only for the harness-assisted route.

**Impact:**

- **Who is affected.** `--show-picker` is the only non-pointer entry into the picker. Keyboard access is an engineering acceptance concern (PRODUCT.md), and M2's keyboard core flow depends on this route.
- **What users see.** A keyboard user launching it from a terminal or Run box usually gets an unfocused picker with misleading guidance.
- **Severity cap.** A workaround exists: Alt+Tab to "ROQA Windows". The pointer route works 5/5. That is why this is major, not blocker.

**Required to resolve:**

1. Make the product's own client hand off its foreground right before signalling. For example, `AllowSetForegroundWindow(ASFW_ANY)` in the `ShowPicker` client path immediately before `SetEvent`, or a resident-PID-specific grant. Verify the actual API contract.
2. Keep a foreground-denial notice visible until focus is gained, and make its guidance keyboard-reachable (for example, mention Alt+Tab).
3. Prove the real route at runtime **without** the harness granting on ROQA's behalf. A harness grant may remain as a separate, labeled control.

### M2B1-OPUS-MIN-1 (minor): the picker's caption and system menu introduce a modal loop that pauses catalog publication and IPC servicing

**Evidence:**

- **Window styles.** The dialog is `WS_POPUP | WS_CAPTION | WS_SYSMENU` (`picker.rs:72`). A caption drag, keyboard move or size, or system menu runs `DefWindowProc`'s modal loop inside `DispatchMessageW` (`host.rs:287`).
- **What pauses.** While that loop runs, the outer loop's `catalog.service()`, `picker.synchronize()` and the IPC event wait (`host.rs:213-239`) do not run.
- **Measured.** With `GUI_INMOVESIZE` set, a live rename was not published for 2 s; it arrived 50 ms after the loop ended (`investigate-5`).
- **Inferred, not run.** By the same mechanism, `--quit` is not serviced until the loop ends.

**Assessment and action:**

- The pause is bounded by explicit user action, ingress stays bounded with an overflow full-scan fallback, and recovery is immediate. That is acceptable for this candidate.
- However, the packet's "No modal loop" claim is literally inaccurate. It does not record the decision the M2a carry-forward asked for.
- **Action:** document the decision for move, size and menu loops. Optionally service deadlines from a `WM_ENTERSIZEMOVE`/`WM_ENTERMENULOOP`-scoped timer, or accept the pause explicitly. I did not test drags of several minutes or quit during a drag.

### M2B1-OPUS-MIN-2 (minor): the new picker harness cannot distinguish a product foreground denial from interference

**Evidence:**

- `Assert-ForegroundOrClassify` (`picker-smoke.ps1:30-38`) reports INTERFERENCE whenever the foreground owner is not the resident.
- When the picker is simply denied and the harness's own fixture stays foreground, which is exactly MAJ-1's failure mode, the result is labeled INCONCLUSIVE/"interference".
- Combined with the harness's own grant, the smoke cannot detect the product-route defect.

**Action:** classify "foreground still the test fixture" separately, for example as `DENIED`. Record the picker status text and last-input time in failure evidence.

### M2B1-OPUS-MIN-3 (minor): foreign-window failures during activation are host-fatal

**Evidence:**

- `activate` uses `ShowWindowAsync(window, SW_RESTORE).ok()?` (`picker.rs:311-313`).
- `picker.message(&message)?` propagates errors through `drain_messages` and `event_loop` (`host.rs:278`), so a failed restore exits the whole resident: dock, notch and catalog, exit code 1.
- **Trigger.** A minimized target destroyed between `valid_target` and `ShowWindowAsync` does this (a narrow TOCTOU). Any failure to restore an elevated or otherwise protected target would do the same.
- The packet discloses only the fail-fast in `Picker::create`.

**Action:** report activation-step failures for a foreign window in the status line instead of terminating the host. Include this path in the M2b2 elevated, packaged and hung matrix. I could not exercise it, because creating an elevated target is out of scope.

### M2B1-OPUS-MIN-4 (minor): the review snapshot does not preserve paths

**Evidence:**

- `m2b1-iteration-1.zip` stores all 117 files by **basename only**, for example `host.rs` instead of `src/host.rs`.
- Three basenames collide (`runtime.log`, `arabic-picker.png`, `picker-smoke.json`), so extraction prompts to overwrite.
- All 117 contents are present by hash, but the zip is not a restorable tree for future diffs as the packet implies. The M2a snapshot kept full paths.

**Action:** regenerate the snapshot with relative paths for the next candidate.

### Non-finding observations (unchanged content or below minor)

- **`relayout` re-centers.** It re-centers the picker on the primary work area for every `WM_SETTINGCHANGE` and `WM_DPICHANGED`, which undoes a user move. This is primary-monitor-first scope, an M3 carry-forward.
- **No unit tests for picker logic.** The filter, the selection-by-key retention and the status logic have no unit tests. The native runs cover them.
- **Reopen while open** loses `return_to`, as noted in the investigation table. Return worked only through z-order.
- **"1 windows."** is a grammar nit.
- **`roqa-uia-probe` is built as a product `[[bin]]`.** It is acceptable for now; revisit it before packaging.
- **The catalog-smoke precondition flake** is described in the runtime table above.

## Checked and acceptable

- **Modeless servicing.** Publication, deadline servicing and IPC continue while the picker is open and idle or focused: the live rename arrived within the run, and the M2a carry-forward holds outside move/menu loops (MIN-1).
- **Handle-less notifications.** The picker does not rely on handle-less foreground notifications.
- **Real rows only.** Rows come only from `WindowModel`. The picker's own window is not listed, and no rows are mocked.
- **FFI safety.**
  - The dialog procedure posts scalar `ACTION`/`RELAYOUT` messages only, under `catch_unwind`; a panic posts `WM_QUIT(71)`, which the loop turns into an orderly error.
  - The `TextScaleFactorChanged` handler captures only an `Arc<AtomicBool>` and a thread ID.
  - The subscription is removed in `Drop` before `RoUninitialize`.
  - The font outlives its child controls: the window is destroyed in `Drop` before the field drops.
- **Activation.**
  - The target is revalidated by PID, `IsWindow`, visibility and not-own-process.
  - `SetForegroundWindow` is attempted only from an explicit Enter, double-click, Open or Alt+O.
  - Escape and Close return the foreground only when the picker owns it.
- **UIA.** The edit control has role 50004 and is named "Filter windows". The list items, the disabled/enabled Open button and the status text are all exposed through the native probe.
- **Text scale.** Only API integration and the font height at 96 DPI with factor 1.0 (`-15`) are verified.

## Not verified (untested by design or out of reach)

- **Untested by design:** Narrator speech, real IME composition and OS text-scale *setting changes*. I make no claim about them.
- **Not exercised:**
  - DPI other than the current one, or a physical scale change;
  - elevated, packaged or hung activation targets;
  - multi-minute drags, or quit during a drag;
  - the `Picker::create` failure path.
- **API contracts not re-fetched.** I relied on long-standing documented foreground-lock behavior and on my empirical results.
- **Machine-specific measurements.** Resource and timing figures are single-machine workload observations, not budgets, leak proofs or performance claims.
- **Independent clean-code-guard.** This is assigned to Sol, and I make no guard claim.

## Final disposition

**REVISE — M2b1 candidate 1, manifest `22923E0D0F08A38CE05B2FCA45BE09DD9FAA2A8B3708269CB8F90B4F08755118` (117 entries).**

| Finding | Severity | Status |
| --- | --- | --- |
| M2B1-OPUS-MAJ-1 | Major | Open; blocks acceptance |
| M2B1-OPUS-MIN-1 to MIN-4 | Minor | Open; nonblocking, may be deferred with a reason |

A fresh independent recheck must verify the MAJ-1 fix on the real `--show-picker` route, without a harness grant made on ROQA's behalf.
