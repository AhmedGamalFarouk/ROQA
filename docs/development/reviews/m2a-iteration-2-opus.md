# M2a iteration 2: independent Opus 5.5 High recheck

- **Reviewer:** Opus 5.5 High, run as a fresh delegated recheck task. I authored none of the M2a source, tests, harness, fixes or docs; `/root` is the author. I implemented nothing, spawned no agents and did not read any iteration-2 Sol output.
- **Scope:** recheck round 1 of **M2a only**. It covers the prior findings, the exact candidate-1→candidate-2 diff and regressions in the changed scheduler and runtime paths. It does not audit unchanged content cosmetically. It does **not** accept full M2 and does not waive the M2b or cumulative stack go/no-go gates.
- **Input manifest:** `docs/development/reviews/m2a-iteration-2.manifest`, SHA-256 **`1E8AEE10CA8DC9C426B096B3AE122609E1BE032F639B55AEC1028A65B7BE2247`**, **71 entries**.

## Disposition: **ACCEPT** (M2a foundation scope only)

- M2A-SOL-MAJ-1 is fixed by construction, and I observed progress under queue-capacity posted traffic.
- OPUS-MIN-1 and OPUS-MIN-2 are resolved.
- OPUS-MIN-4 is partially resolved; the remainder is an open minor.
- OPUS-MIN-3 and SOL-MIN-1 are acceptably deferred as nonblocking minors.
- No blocker or major finding is open, and I found no new finding at minor severity or above.
- **This acceptance is mine only.** M2a still needs Sol's fresh acceptance on this same manifest.

## Input integrity

- **Manifest entries:** all 71 matched at start and again at end, after every native run. Records: `artifacts/m2a-opus-recheck-2/manifest-start.json` and `manifest-end.json`. The manifest's own hash matched at start and end.
- **Old nested manifests:** treated only as superseded baseline context, not as integrity expectations.
- **Candidate-2 snapshot:** `artifacts/review-snapshots/m2a-iteration-2.zip` has SHA-256 `11D73C326D8919BDB83057752D4BEC096AB9352C51EC7FAE43089F66FF7F22DE` at start and end. It holds 72 entries: all 71 manifest entries, each with a matching hash, plus the manifest itself.
- **Candidate-1 snapshot:** `artifacts/review-snapshots/m2a-iteration-1.zip` is `AD637B9DCD402A09AAF7E7420003DDFDC4C82CD1CCF40C55131E02A5F08643D0` at start and end. It still matches my iteration-1 record.
- **Frozen runtime binary:** `target/release/roqa.exe` is `779D62B0567FA049CA80BAD9B771542B92849E0920F7592373230C8E94EF2988` at start and end. Every evidence JSON I produced records this same hash.
- **Diff verification:** I extracted candidate 1 into an ignored scratch directory and ran my own `git diff --no-index`. The changed files are `src/window_catalog.rs` (+165/−62), `src/host.rs` (+8/−5), `src/main.rs` (+1), `tests/NativeProbe.cs` (+24), `scripts/catalog-smoke.ps1` (+17) and `README.md` (+2/−2), plus the new `src/catalog_timing.rs`. This agrees with `artifacts/m2a-candidate-2/candidate-diff.txt`.
- **Unchanged files:** these files are byte-identical to candidate 1, confirming the "unchanged" claim:
  - `graphics.rs`, `ipc.rs`, `layout.rs`, `native_window.rs`, `window_model.rs` and `window_source.rs`
  - `Cargo.toml`, `Cargo.lock`, `PLAN.md`, `build.rs`, `app.manifest` and `rust-toolchain.toml`
  - `smoke.ps1`, `measure-idle.ps1`, `verify.ps1` and `check.ps1`
- The scratch extraction was deleted after the comparison.
- I did not write to source, `target/release`, the manifest, frozen evidence or `artifacts/m2a-candidate-2/`. I made no commit, install or global, registry or taskbar change, and killed no process. No ROQA process existed before or after any run.

## Independent runtime evidence (exclusive native slot; frozen scripts unmodified; run sequentially)

| Run | Result | Key observations |
| --- | --- | --- |
| `scripts/catalog-smoke.ps1 -SkipBuild -EvidenceDirectory artifacts/m2a-opus-recheck-2/catalog`, fresh `powershell -NoProfile` | **PASS 67**, exit 0, 44.7 s | **Queue traffic:** **3,119,383 posts succeeded, 51,700 were rejected at capacity**. With the producer still alive, ROQA published the changed fixture metadata *and* completed a periodic repair; afterwards generation was 613 and reconciliations 1.<br>**Title storm:** 501 title changes → **6 targeted batches, 6 inspections, 0 full scans**; convergence was observed at 373.3 ms.<br>**Resources:** warm and blocks 1–2 at 11,161,600 B / 252 handles / 14 USER / 0 GDI; block 3 at 11,190,272 B, other counts the same.<br>**Quit:** 213.5 ms with the storm still running.<br>**Final state:** `stopped:true`, `hooks_live:0`, `full_scans:2` (startup + repair), `delta_batches:51`, `discarded:0`. `runtime.log` is empty. |
| `scripts/smoke.ps1 -SkipBuild .../shell`, then `.../shell-reentry`, both in the **same** host (pid 9328, runner `run-shell-twice.ps1`) | **PASS 305 / PASS 305**, exit 0 | Every capture was stable on attempt 1. **Resources (flat):** run 1 at 11,247,616 B / 255 handles / 14 USER / 0 GDI; run 2 at 11,304,960 B / 259 / 14 / 0.<br>**Image consistency:** the six PNG hashes are identical between my two runs. Each idle image equals its leave image.<br>**Comparison with author:** center pixels match the author's captures (dock-hover center 42,51,62; notch-idle center 22,27,34). The corner pixels differ only in desktop background, which falls inside the harness's excluded 18-px corners.<br>**Visual check:** I inspected `dock-hover.png` and `notch-idle.png`. They show the expected M1 rounded capsules, with no regression. |
| `scripts/measure-idle.ps1 -SkipBuild .../idle` (optional) | Completed | **30.015 s**, CPU **0 ms at endpoint-counter resolution**. Private bytes 11,218,944 → 11,411,456; handles 231 → 234. This is consistent with the deadline-bounded wait not spinning. It is a single observation, not a budget, zero-use proof or M5 result. |
| `scripts/check.ps1` with `CARGO_TARGET_DIR=artifacts/m2a-opus-recheck-2/target` and offline cargo | exit **0** | fmt, all-target warnings-denied Clippy, and **11/11 tests** passed. The 3 new `catalog_timing` tests and the new `window_catalog` service test are included. Streams are in `checks/`. The frozen binary was not rebuilt; I made no byte-reproducibility claim. |

No input interference occurred, so no run was INCONCLUSIVE and none needed a retry.

## Prior findings: recheck status

### M2A-SOL-MAJ-1 (WM_TIMER starvation): **resolved for M2a**

- **Timer removal.** `SetTimer` and `KillTimer` are gone from the catalog, with no `WM_TIMER` arm, and `INFINITE` is no longer used. `host::event_loop` (`src/host.rs:203-224`) calls `catalog.service()` before every `MsgWaitForMultipleObjectsEx`. The timeout is `catalog.wait_milliseconds()`, which is always ≤ the 30-s repair deadline and is rounded *up* (`src/catalog_timing.rs:56-65`), so it cannot busy-spin. The idle run confirms 0 ms of CPU.
- **Service runs under sustained traffic.** `MWMO_INPUTAVAILABLE` returns immediately, `drain_messages` stops after 128 messages or 4 ms, and the loop returns to `service()`. Elapsed settle, retry and repair deadlines are therefore evaluated at least once per bounded pass, whatever the queue priority. My native run reached queue capacity (51,700 rejected posts). It still showed both a metadata publication and a repair while the producer was alive.
- **Limit (stated honestly by the author, and confirmed):** this proves progress under that workload. It does not prove the queue was continuously non-empty, and it does not re-run candidate 1 to reproduce the original stall.
- **Batching and in-flight invariants hold:**
  - `arm` uses `get_or_insert`, so new events cannot slide the 50-ms deadline.
  - `refresh_due` consumes the deadline even if `ScanSchedule::request` returns `None` while a request is in flight. This prevents a zero-timeout spin.
  - The follow-up is re-armed in `collect` only when `has_pending` (dirty and not in flight).
  - One in-flight request and capacity-one channels are unchanged.
- **A disconnected result channel is still an error** on every service pass, not only at the 30-s fallback. The unit test proves it.

### M2A-OPUS-MIN-1 (DIRTY latch never self-heals): **resolved**

- **Shared drain.** `take_ingress` (`src/window_catalog.rs:375-386`) resets `NOTIFIED`, swaps `HANDLE_COUNT`/`OVERFLOW` and drains the slots. It runs in every `service_at` pass, as well as on `DIRTY`, and treats `count > RING_CAPACITY` as overflow.
- **Collection does not depend on COMPLETE.** `collect` runs on every pass, so a lost `COMPLETE` no longer blocks `in_flight` until the next 30-s timer.
- **Recovery is faster than the packet claims.** Out-of-context callbacks run only while this thread retrieves messages. After a nested or modal pump returns, or after a rejected `DIRTY` post inside `drain_messages`, the next outer pass recovers immediately. The 30-s figure is only a worst case for a wake lost while the thread is otherwise waiting.
- **Same-thread invariant.** The swap and drain run on the registering UI thread without pumping messages, as the comment states.
- **M2b note (not a finding):** while a future modal loop is active, `service()` does not run, so publication pauses until the loop exits. Ingress stays bounded and escalates to a full scan on overflow. M2b should decide whether that pause is acceptable for menus and drags.

### M2A-OPUS-MIN-2 (incomplete snapshots: no follow-up or backoff): **resolved**

- **Retry schedule.** `completed(false)` caps `failures` at 6, which gives retries at 1, 2, 4, 8, 16 and then 30 s (`1<<5 = 32` is clamped to 30). It also arms settle.
- **Retry floor.** `collect` sets `full = true` and invalidates the schedule. `refresh_deadline = max(settle, retry)`, so neither new events nor the repair deadline can bypass the retry floor.
- **Reset on success.** A complete result clears both `failures` and `retry`.
- **Last complete model kept.** `WindowModel::apply` is unchanged and still keeps the last complete model.
- **Test coverage.** The clock test and the service test (no retry at 1099 ms, a full retry at 1100 ms) cover this.
- **Limit (not a finding):** the provisional or additive publication of truncated scans that I suggested optionally was not adopted. It is not required.

### M2A-OPUS-MIN-4 (no coverage below the harness): **partially resolved; remains open minor, nonblocking**

The new `window_catalog` test drives the real `Catalog::service_at` with real channels and injected scalar ingress. It covers:

- a stuck `NOTIFIED`, and ring overflow escalating to a full request with 256 handles;
- a lost `COMPLETE`, leading to publication;
- the 50-ms follow-up;
- incomplete results, leading to a timed full retry;
- periodic repair;
- disconnect, leading to an error.

The callback statics are owned by this single test, which is safe under parallel test threads because no other test touches them. **Still untested:**

- the `SourceState::reconcile` delta/cap/`needs_full` branches;
- native source fault injection;
- real lost-WinEvent cases.

The author says so honestly. Carry this to M2b, before any change to the source cache.

### M2A-OPUS-MIN-3 (diagnostics write failure is fatal): **deferred, accepted as nonblocking**

- The code is unchanged, and it is diagnostics-only: normal launches write nothing.
- `collect` now runs on every pass, so a diagnostics write also happens on each publication found through service. Frequency is still bounded by publications.
- It remains open for M2b or the stack gate.

### M2A-SOL-MIN-1 (constructor decomposition): **deferred, nonblocking**

- The constructor is smaller than before, because timer creation was removed.
- Sol owns its status.

## Regression observations on the diff (no finding ID)

- **Null-HWND events.** In candidate 1, `DIRTY` always invalidated, so a wake whose ring held only zero HWNDs triggered a targeted batch with no handles. That batch still re-flagged `foreground` across cached rows (`window_source.rs:249`). Candidate 2's `drain_ingress` returns early when it sees only zero handles and no overflow. Any foreground re-flag therefore waits for the next event or repair. Hooked system events normally carry a non-null HWND, and the next real event or the 30-s repair converges. I consider this immaterial for M2a. It is worth keeping in mind if M2b relies on null-HWND foreground transitions.
- **Harness side effects.** The queue probe posts `WM_APP+50` (0x8032) thread messages. `Catalog::message` declines them and dispatching a null-HWND message is harmless. The PASS result and exit code 0 confirm this.
- **Harness cleanup.** The producer is joined both on the success path and in `finally`, with a 40-s join. It targets only the ROQA dock's own thread.
- **README.** The README change accurately describes the new deadline behavior. It does not overstate the queue test.

## Limits of this recheck

- **No clean-code-guard pass.** That pass is assigned to Sol. The new production guard work is Sol's; I only inspected it.
- **Primary API contracts not re-fetched.** I did not re-fetch the Microsoft `MsgWaitForMultipleObjectsEx`, `WM_TIMER` or `PostThreadMessage` contracts. I relied on the author's `api-contracts.json`, Sol's iteration-1 citations and long-standing documented behavior.
- **Native cases not exercised:**
  - real modal-loop message loss, forced native WinEvent loss or overflow, and native source API fault injection;
  - a hung source;
  - packaged, elevated or UIAccess windows;
  - physical DPI or topology changes.
- **Machine-specific measurements.** Resource and idle numbers are single-workload observations on this machine (Ryzen 5 PRO 4650G, Radeon iGPU, 1920×1080 @ 144 Hz, Windows 11 26200). They are not leak-freedom proofs or budget verdicts.
- **Gates still open:**
  - **M2b:** icons, stable packaged identity, activation with target revalidation, keyboard, UIA/Narrator, actual Arabic/RTL rendering, system text scaling and IME.
  - **Cumulative:** the full-M2 Sol + Opus go/no-go on one identical manifest before M3.

**Final disposition: ACCEPT M2a foundation on manifest `1E8AEE10CA8DC9C426B096B3AE122609E1BE032F639B55AEC1028A65B7BE2247` (71 entries).**

| Finding | Status |
| --- | --- |
| SOL-MAJ-1 | Resolved |
| OPUS-MIN-1 | Resolved |
| OPUS-MIN-2 | Resolved |
| OPUS-MIN-4 | Partial, open minor |
| OPUS-MIN-3 | Deferred minor |
| SOL-MIN-1 | Deferred minor |

M2a acceptance still requires Sol's independent ACCEPT on this exact manifest.
