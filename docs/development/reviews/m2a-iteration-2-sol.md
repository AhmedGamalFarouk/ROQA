# Code review: M2a iteration 2 independent Sol High recheck

## Summary

**ACCEPT the M2a foundation correction on the exact manifest below.** The prior major scheduling defect is fixed: catalog progress no longer depends on delivery of low-priority WM_TIMER messages. This is one independent council disposition, not combined milestone acceptance and not acceptance of M2b, full M2, or the stack go/no-go.

Project counts: **0 blockers, 0 majors, 3 carried nonblocking minors** (constructor decomposition, opt-in diagnostics policy, and remaining coverage). No new blocking finding. Guard-only counts: **0 critical, 0 important, 1 carried nit**.

- Date: 2026-11-09. Reviewer: fresh independent GPT-6.1 Sol High task. Candidate author: `/root`, including all candidate production, tests and documentation. I authored none of those inputs and delegated nothing.
- Scope: prior Sol/Opus findings, exact candidate-1-to-2 diff, and resulting regressions. I read AGENTS.md, PLAN.md, PRODUCT.md, the mutable progress ledger, candidate-2 packet and both original M2a reports. No new cosmetic audit of unchanged code.
- Input manifest: `docs/development/reviews/m2a-iteration-2.manifest`, **71 entries**, SHA-256 **`1E8AEE10CA8DC9C426B096B3AE122609E1BE032F639B55AEC1028A65B7BE2247`**.
- No GUI/native scenario launches by this reviewer; Opus owns that exclusive slot. No production/test/script/manifest/frozen-evidence changes, installations, commits or global configuration changes. Reviewer-only scratch tests and evidence are under `artifacts/review-sol-m2a-iteration-2/`; the only non-ignored output is this report.

## Input integrity and exact diff

**71/71 current outer manifest entries matched before and after review, with zero mismatches.** All 71 also match the candidate-2 snapshot entries. Historical manifests, packets and reports are hashed context documents, not recursive current-source expectations. The mutable ledger and new reviewer outputs are outside this input manifest.

Preserved identities, checked at start and end:

| Input | SHA-256 |
| --- | --- |
| Frozen `target/release/roqa.exe` | `779D62B0567FA049CA80BAD9B771542B92849E0920F7592373230C8E94EF2988` |
| Candidate-1 snapshot zip | `AD637B9DCD402A09AAF7E7420003DDFDC4C82CD1CCF40C55131E02A5F08643D0` |
| Candidate-2 snapshot zip | `11D73C326D8919BDB83057752D4BEC096AB9352C51EC7FAE43089F66FF7F22DE` |
| Frozen `candidate-diff.txt` | `22AD3BFA9C7E0A239A65794A2FE2E8292299FC30D8D43F0AB063464E23EE17F7` |

Comparing the two archives identifies exactly six changed shared files: `src/window_catalog.rs`, `src/host.rs`, `src/main.rs`, `tests/NativeProbe.cs`, `scripts/catalog-smoke.ps1` and README.md; `src/catalog_timing.rs` is new. New evidence/packet/report entries and omitted superseded evidence are separately recorded in `archive-delta.json`. Source scanning, model, rendering, IPC, dependency/lock/toolchain and normative PLAN files are byte-unchanged between snapshots.

The packet's exact textual diff was independently reproduced, including its line-ending representation: raw archive baseline, universal-line-ending read of the candidate, then Windows output reproduce the CRCRLF baseline lines. This is not an extra semantic change. `normalized-diff.txt` provides an easier-to-audit semantic view. Evidence: `manifest-before.json`, `manifest-after.json`, `archive-verification.json`, `archive-delta.json`, and `identities-after.json` in the reviewer directory.

## Prior finding dispositions

### M2A-SOL-MAJ-1: fixed / resolved for this candidate

`src/catalog_timing.rs:21` arms the first event's 50 ms settle deadline with `get_or_insert`; later events cannot slide it. `:24` checks and advances the 30 s repair deadline independently. `:45` consumes an elapsed refresh deadline once. `:56` selects the earlier repair/refresh deadline, saturates overdue durations to zero and rounds positive sub-millisecond remainders upward.

`src/host.rs:204` calls catalog service before every wait/drain pass; `:209` supplies the computed timeout instead of INFINITE. The existing 128-message/4 ms drain at `:233` returns to that service even while posted traffic remains. With current construction/servicing, the repair deadline bounds the wait to at most 30,000 ms; the generic integer clamp cannot reach the INFINITE sentinel through this state. Timeout returns still pass through bounded message handling and back to service. No WM_TIMER/SetTimer/KillTimer dependency remains in catalog scheduling.

`src/window_catalog.rs:236` orders ingress drain, result collection, elapsed repair invalidation, then due request. A repair adds intent rather than bypassing the single-flight gate. `ScanSchedule::request` remains the authority for one in-flight generation; request/result channels remain capacity one. If a due refresh is consumed while a request is in flight, dirty/full/pending state survives; completion re-arms a follow-up batch. Complete in-order results remain publishable despite newer invalidations, avoiding publication starvation.

The author supplied a real posted-traffic scenario and regression evidence; this reviewer independently passed the deterministic/service tests and additional timing-boundary probes described below. The native evidence is qualified rather than promoted into proof of a continuously nonempty queue or reproduction of the original fault. The source-level forward-progress defect is closed.

### M2A-OPUS-MIN-1: fixed / resolved in the scoped recovery sense

`src/window_catalog.rs:238` drains ingress on every service, independently of DIRTY; `:239` collects results independently of COMPLETE. `take_ingress` at `:375` resets NOTIFIED, swaps count/overflow, consumes bounded slots and also escalates an oversized count to full repair. Empty or duplicate wakes do not fabricate invalidation work.

No allocation/native message pumping occurs inside the unchanged scalar FFI callback, and the extracted drain does not pump messages. Out-of-context delivery and drain remain on the registering UI thread. A lost wake can no longer permanently strand the latch or result. In an otherwise idle loop, recovery can still wait until the finite repair wake, up to roughly the next 30-second running interval plus normal service/settle/worker time. This is bounded recovery, not 50 ms latency after a lost notification. Actual modal-loop loss was not injected, and uninterrupted progress while trapped inside an arbitrary future modal loop is not certified.

### M2A-OPUS-MIN-2: fixed / resolved

`src/catalog_timing.rs:31` increments a capped failure count and schedules 1/2/4/8/16/30-second retry floors; a complete result clears the floor/count. `refresh_deadline` at `:41` takes `max(settle, retry)`. Thus neither new invalidations nor a coincident repair can bypass backoff. They also cannot postpone an already-armed deadline indefinitely.

`src/window_catalog.rs:288` applies only the matching completion; incomplete results retain the last complete model, set full intent and invalidate follow-up work. The source's unchanged `needs_full` behavior and the UI full flag both lead the retry to full acquisition. Persistent incompleteness remains uncertain, with bounded retries, not manufactured complete records. Successful completion resumes ordinary event batching. Independent crossing-repair and reset probes passed.

## Nits

- **M2A-SOL-MIN-1: deferred/nonblocking, unchanged in substance.** `src/window_catalog.rs:113` still embeds channel/worker and hook setup in construction (`:143`, `:175`). Guard principle: small functions / one abstraction level. Optional fix: extract narrowly named worker-startup and subscription helpers while retaining the partially initialized Catalog as RAII owner. This recheck adds no new style finding on that unchanged lifecycle design.
- **M2A-OPUS-MIN-3: deferred/nonblocking.** `src/window_catalog.rs:296` still propagates an opt-in diagnostic write failure; `:355` writes then renames synchronously. Default resident launches have no diagnostic writes. Documented fail-fast policy is honest but transient locks can terminate evidence-mode runs. Optional follow-up: bounded nonfatal intermediate-write retries/error accounting, retaining strict final evidence handling. I did not inject sharing failures or certify them fixed.
- **M2A-OPUS-MIN-4: partially addressed, remaining coverage open/nonblocking.** New tests exercise real Catalog service/channels and injected scalar ingress, not just the model. They do not execute `SourceState::reconcile` cache mutation/fault branches, native callback overflow, OS failure, or actual modal lost-message paths. Add bounded source/cache fault tests and explicit native missed-event repair assertions in the next reliability work. No full-source-fault coverage claim is accepted.

## What's good

- Timing policy is now a small dependency-free unit separated from native lifetime ownership; the host explicitly services deadlines instead of relying on message priority.
- Single-flight, bounded ingress/queues, preserved complete models and explicit error propagation survive the correction.
- Teardown still clears TARGET, unhooks, cancels, drops both channel ends (including the result receiver before join), joins the worker and only then records stopped. Timer removal eliminates ownership rather than leaving orphaned timer paths.

## Coverage: independent clean-code-guard

Applied **review mode**, not implementation/refactoring mode, using `C:/Users/Ahmed/.agents/skills/clean-code-guard/SKILL.md` and relevant `references/ai-failure-modes.md`, `review-checklist.md`, `naming-and-functions.md`, `dry-kiss-yagni.md` and SRP guidance in `solid.md`. Walked changed nontrivial production code against neighboring model/source/host lifetime contracts. Cosmetic-only concerns were not promoted into new recheck findings.

- Section A (naming & functions): carried **M2A-SOL-MIN-1** only; the timing unit and extracted ingress path make the changed scheduling responsibilities traceable. Native ABI parameters are not redesignable configuration APIs.
- Section B (comments & formatting): **clean for scoped review**; safety/non-reentrancy/rounding comments explain actual invariants. Formatter independently passes.
- Section C (SOLID): **clean**; timing, native catalog lifetime, source acquisition/cache and model publication have coherent boundaries. No speculative backend interface or factory was introduced.
- Section D (DRY/KISS/YAGNI): **clean**; one clock policy and one service path, with no duplicated timer fallback, new dependency or speculative configuration.
- Section E (AI failure modes): prior **M2A-SOL-MAJ-1 resolved**. No swallowed worker disconnection, fake production success, stale timer branch or removed cleanup/boundary check found. Incomplete scans are explicitly uncertain. Compile success is not used as proof of native-fault or accessibility behavior.

**clean-code-guard: 0 fixed, 1 flagged for author (carried nonblocking constructor minor); no new guard finding.** I implemented no candidate fix. My scratch timing tests are reviewer evidence, not candidate implementation.

## Independent checks and their actual strength

Ran project verification with process-local `CARGO_NET_OFFLINE=true`, `RUSTUP_OFFLINE=1`, and `CARGO_TARGET_DIR=artifacts/review-sol-m2a-iteration-2/target`, through unchanged `scripts/verify.ps1 -EvidenceDirectory artifacts/review-sol-m2a-iteration-2/verification`.

- **PASS:** fmt check; locked all-target Clippy with warnings denied; **11/11 unit tests**; locked release build. Verification child steps and enclosing execution exited **0**. Commands, timestamps and separate stdout/stderr are retained in `verification/`.
- Rebuilt scratch release SHA-256: `5FE38D5132B0A6B833822F30E5B00C5A23DDECF07F3599108392C33C47E9D5E7`. It was **not run**, is not claimed byte-reproducible, and did not replace the frozen runtime binary.
- The three candidate timing tests prove non-sliding event deadlines, backoff progression/reset, rounding and follow-up batching through direct clock advancement. They do not generate a real Windows queue backlog.
- The Catalog test at `src/window_catalog.rs:392` really constructs capacity-one channels, injects overflow/lost latch state, calls production `service_at`, observes full request/publication/follow-up/retry/repair and detects result disconnection without a timer message. It substitutes snapshots and owns no native worker/hooks. Combined with unchanged ScanSchedule tests, this supports coalescing/single-flight behavior, not native source faults.
- Added three reviewer-only boundary probes importing the **unchanged** production timing module: repair at 30 s cannot bypass a retry due at 31 s despite 1,000 intervening event arms; late service coalesces overdue repair without a catch-up busy loop; successful completion resets retry without erasing a pending settle. Scratch rustc test harness: **6/6 PASS** (three added probes plus the production module's three tests), compile/run exit **0**. Source and streams retained as `timing-probes.rs` and `timing-probes*.log`.
- All **9 PowerShell scripts** parse with zero errors (`powershell-parse.json`). NativeProbe C# compiled via Add-Type; no fixture, DPI, window or traffic method was invoked. `check-summary.json` records these results.

Primary contracts independently rechecked: Microsoft Learn **MsgWaitForMultipleObjectsEx** (finite timeout, old unread input with MWMO_INPUTAVAILABLE), **SetWinEventHook** (out-of-context registering-thread delivery), **PostThreadMessageW** (modal loss and failure return), and **WM_TIMER** (low priority); Rust std **Instant** documentation (monotonic measurement, with platform/suspend qualifications). The version-specific Rust 1.91.1 documentation URL was unavailable through the fetch tool; the actual pinned compiler successfully built the candidate and timing probes. This does not certify suspend/resume timing. No new Windows API/dependency was introduced by the correction.

## Frozen author runtime evidence: inspected, not independently rerun

- Catalog JSON: **PASS 67**, matching frozen binary identity; 501 title changes yielded 5 targeted batches, 5 inspections and 0 full scans. Four samples are 11,206,656 private bytes / 248 handles / 14 USER / 0 GDI. Final diagnostics show 2 full scans, 1 reconciliation and zero hooks/stopped after cleanup. Author storm-quit observation: 183.1402 ms.
- Posted-traffic evidence: **3,495,344 successful and 70,655 rejected posts** during the bounded 36 s producer. The predicates verify changed metadata and a serviced repair deadline in complete snapshots while the producer is alive. They do **not** prove continuously nonempty queue occupancy or retroactively reproduce the original timer defect.
- Precision limits in this harness: `tests/NativeProbe.cs:128` counts any failed post without retaining GetLastError; capacity pressure is consistent with the observations, not independently classified per rejection. `scripts/catalog-smoke.ps1:115` checks the reconciliation counter, not a full-scan counter increase. A previously in-flight targeted completion could carry that counter before full repair finishes. The final full-scan count proves eventual repair, but the retained queue sample alone does not uniquely prove full-repair completion while the producer remained alive. To strengthen the existing coverage follow-up, record/assert a baseline-relative full-scan increase and retain rejection codes. These qualifications do not reopen the corrected source scheduling mechanism or block this foundation.
- Shell and shell-reentry JSON: **PASS 305 each**, same binary. Same-PowerShell-host execution remains author-reported; I did not rerun scenarios, inspect captures anew or independently establish that provenance.
- Idle JSON: 30.0163 s, CPU delta 0 ms at endpoint-counter resolution, private bytes 10,997,760 to 11,198,464. Not zero resource use, not a budget/comparison result, and no DWM/GPU/power attribution. No new performance measurement by me.

## Limits and final disposition

No reviewer-native run, physical DPI/topology change, modal loss, native overflow/missed-event injection, source/cache/API failure injection, hung-source teardown, packaged/elevated/inaccessible matrix, or accessibility/text-input validation is certified here. Cooperative source cancellation/time checks are not hard OS-call or shutdown deadlines. Ownership/FFI review found no regression: callbacks remain scalar/non-panicking or panic-contained at their existing boundary; COM/resources remain on their existing owners; explicit worker errors still reach orderly shutdown.

M2b still requires real icons, stable packaged identity, activation/target revalidation, keyboard, UIA/Narrator, actual Arabic/RTL rendering, system text scaling and an IME-capable editable route. Full M2 requires both fresh council ACCEPT dispositions on the same final manifest and cumulative stack go/no-go before M3. Nothing in this report waives those gates or supplies the other reviewer's result.

**Final Sol disposition: ACCEPT M2a foundation correction, manifest `1E8AEE10CA8DC9C426B096B3AE122609E1BE032F639B55AEC1028A65B7BE2247`. M2A-SOL-MAJ-1, M2A-OPUS-MIN-1 and M2A-OPUS-MIN-2 are resolved for these inputs; the three listed minors remain nonblocking.**
