# Code review: M2a iteration 1 catalog foundation

## Summary

**REVISE the foundation candidate.** Its bounded worker, source cache and cleanup are reasonable, but refresh/repair scheduling can stop indefinitely under sustained higher-priority UI queue traffic. This does not accept full M2 or its stack go/no-go.

Counts: **1 critical correctness finding** (project **major**), **0 important findings**, **1 nit** (project **minor**). Project totals: **0 blockers, 1 major, 1 minor**. Only the major blocks.

- Author: `/root`, all M2a implementation/fixes/tests/docs. Reviewer: fresh independent Sol High task, 2026-11-09, with no implementation authorship or delegation. I did not read the other M2a review.
- Exact input: `docs/development/reviews/m2a-iteration-1.manifest`, SHA-256 **`C493F3615CBE4CEE643E3264D67C50961B09E44CD0C48A96E418A0A5BDAFBBAC`**, **61 inputs**.
- Every current outer entry matches at start/end. Independent records: `artifacts/review-sol-m2a-iteration-1/manifest-start.json` and `manifest-end.json` in that directory. All 61 also match the local snapshot zip; see `archive-verification.json` there.
- M1 was accepted by both on `3080840DA84AE480AF07B382ABD4DF42D9E2E1956706EEAACB9693E3606E0D7C` before M2. Nested historical manifests are baseline context, not integrity expectations for superseding M2 source. The ledger is administrative, not a frozen acceptance authority.

## Critical findings

### M2A-SOL-MAJ-1: refresh and repair depend on starvation-prone WM_TIMER delivery

**Severity: major. Status: open.** Foundation correctness/forward-progress defect.

Source evidence:

- `src/window_catalog.rs:212`: DIRTY drains ingress, invalidates and calls `self.arm_settle()?`, without servicing an elapsed deadline.
- `src/window_catalog.rs:229`: event-driven requests subsequently start only in `WM_TIMER if Some(message.wParam.0) == self.settle`, through `self.request()?`.
- `src/window_catalog.rs:233`: 30-second repair/result collection also needs WM_TIMER.
- `src/window_catalog.rs:291`: completion with pending work only arms another settle timer.
- `src/window_catalog.rs:368`: timers use `SetTimer(None, 0, milliseconds, None)`.
- `src/host.rs:209`: `INFINITE` wait has no catalog deadline/independent timer handle.
- `src/host.rs:230`: bounded drains still use unfiltered `PeekMessageW(..., 0, 0, PM_REMOVE)`. Yielding permits IPC checks, not guaranteed timer service.

Microsoft documents WM_TIMER as low priority: unfiltered GetMessage/PeekMessage generate it only without higher-priority messages remaining. If sustained notifications and posted DIRTY wakes keep the queue nonempty, elapsed time cannot reach either timer branch. Pending changes can stop publishing after startup or the last completion; overflow can set full-repair intent without ever starting repair. The 30-second fallback and disconnected-worker detection at that fallback share the same starvation condition.

This is a **source/API-derived failure condition**, not a reproduced desktop stall. It requires sustained backlog, not simply any continuous events. `tests/NativeProbe.cs:115` sleeps between storm changes; the reducer test calls `request()` directly. Their recorded publication/quit success is valid for those workloads but does not close this hole.

Primary contracts checked: Microsoft Learn **WM_TIMER message**, remarks (`https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-timer`), and **PeekMessageW**, remarks (`https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-peekmessagew`). They specify low timer priority/default queue order. Application-owned memory can remain bounded while repair/publication makes no progress.

Minimal fix: retain 50-ms batching but maintain monotonic settle/repair deadlines, service due work on each bounded outer-loop pass, and bound the blocking wait by the next deadline. An integrated waitable timer is another option. Preserve one in-flight request/coalescing; do not scan per event or bypass follow-up batching.

Recheck: deterministic tests must advance time with higher-priority traffic present and no WM_TIMER delivery, proving refresh, debounced follow-up and repair/result-disconnect paths remain reachable. Add an owned-fixture saturated-queue native check in the exclusive runtime slot and rerun catalog/shell regressions on newly frozen inputs. I implemented no fix or native experiment.

## Nits

### M2A-SOL-MIN-1: constructor embeds distinct worker and hook lifecycles

**Severity: minor. Status: open/nonblocking.** Guard section A, function size/abstraction levels.

`src/window_catalog.rs:114` combines ownership reservation, channels, worker loop, registrations, timers and initial request in roughly 90 lines. Worker startup at `src/window_catalog.rs:149` and hook installation at `src/window_catalog.rs:177` are separately nameable lifecycle units, making partial-startup auditing harder.

Optional fix: extract narrow worker-startup/hook-installation helpers while preserving the partially initialized Catalog as RAII owner and retaining failure cleanup order. No architectural rewrite requested; this minor never blocks acceptance.

## What's good

- Scalar-only ingress, a fixed buffer/capped pending set, capacity-one channels and one worker avoid per-event full-census amplification. Out-of-context delivery is same-thread; this callback does not call message-pumping or foreign COM operations that would make bookkeeping reentrant.
- Complete in-order snapshots publish while newer invalidations remain pending. Incomplete snapshots preserve the last complete model; incomplete source work forces the next reconciliation through a full scan rather than publishing a partially mutated cache as complete.
- Teardown disables ingress, unhooks on the registering thread, stops timers, cancels, drops channels and joins. Dropping the result receiver before joining releases a worker blocked on its bounded send. Unexpected result disconnection is an error, not successful empty collection.

## Coverage

Independent **clean-code-guard review mode** applied using `C:/Users/Ahmed/.agents/skills/clean-code-guard/SKILL.md`, `references/ai-failure-modes.md` and `references/review-checklist.md`. This is a correctness review, not a behavior-preserving refactor. Skill severities are mapped to project severities above.

- Section A (naming & functions): **M2A-SOL-MIN-1**, multiple abstraction levels in construction. FFI callback parameters/native-boundary guards are not speculative API design.
- Section B (comments & formatting): **clean** for review purposes; native ownership/safety comments are relevant, with no material misleading scaffolding/style inconsistency. Formatter results are separate.
- Section C (SOLID): **clean**; Catalog owns ingress/scheduling/lifetime, SourceState acquisition/cache, WindowModel publication/grouping. No speculative swappable-backend interface; opt-in diagnostics has a current caller.
- Section D (DRY/KISS/YAGNI): **clean**; full/targeted acquisition intentionally have different cache semantics, one worker/model is appropriate, and no unnecessary dependency was added.
- Section E (AI failure modes): **M2A-SOL-MAJ-1**, plausible but incorrect external-API scheduling assumption. No fake app list, hardcoded successful fallback or removed ordinary cleanup path found. Enumeration panic containment marks incompleteness; documented filtering/PID fallback are not fabricated enumeration success.

**clean-code-guard: 0 fixed, 2 flagged for author (1 major correctness, 1 nonblocking minor).** No implementation changes by this reviewer.

## Correctness and ownership audit

- **Ingress/FFI:** Six narrow out-of-context, skip-own-process event ranges are state notifications, not input interception/injected code. Object notifications filter window/self IDs. The callback uses scalar atomics/bounded indexing/a posted wake, without allocation or foreign title queries. EnumWindows synchronously borrows stack state, catches panic before returning through FFI, and records incompleteness. Microsoft SetWinEventHook, UnhookWinEvent and hook-reentrancy contracts checked. Current registration/message processing/unhook remain on the UI thread.
- **Bounds:** Enumeration caps at 4096 raw HWNDs; pending/retained windows cap at 512. Batches check cancellation and elapsed 250-ms intervals between native inspections. This is cooperative, not a hard native-call/shutdown deadline. The major concerns reaching repair, not an unbounded Rust queue.
- **Acquisition/identity:** Root/visibility/style/owner/cloak checks match the disclosed filter. Own PID is excluded before caption retrieval; captions bounded, HWND/PID rechecked, limited-rights process handles owned/closed. Failed image queries fall back to PID without elevation. Microsoft GetWindowTextW's foreign-caption contract supports avoiding foreign WM_GETTEXT sends. No foreign SendMessage icon/title query introduced. Cross-process metadata is not atomic; M2b must revalidate identity before activation.
- **Partial/stale/cache:** Incomplete full scans preserve the source's prior complete cache; incomplete targeted work sets `needs_full`. The UI preserves complete records and marks uncertainty. Matching completion clears only the single in-flight slot; intervening dirty state survives. Old/duplicate generations are rejected. No additional cache-consistency defect found; actual failure injection remains unperformed.
- **Shutdown/errors:** Startup owns installed hooks/worker before later fallible operations. Explicit shutdown executes even after event-loop error, with Drop fallback. The worker owns no UI/COM objects and its selected APIs require no COM apartment. Diagnostics/explicit hook/timer/join failures propagate. No taskbar/work-area/startup/privilege/registry mutation introduced.
- **Provenance:** Toolchain/dependencies remain pinned with the local wrapper/windows 0.62.2. No imported Nectar/Nusa implementation/assets found in the additions; source inspection is not legal authorship certification.

## Independent checks and captured evidence

### Checks actually run

Used a separate ignored target directory, without starting ROQA or GUI/smoke/idle scenarios:

```powershell
$env:CARGO_NET_OFFLINE = 'true'
$env:CARGO_TARGET_DIR = '<project>/artifacts/review-sol-m2a-iteration-1/target'
./scripts/verify.ps1 -EvidenceDirectory artifacts/review-sol-m2a-iteration-1/verification
```

- Formatter check, locked all-target Clippy with warnings denied, locked tests (**7 passed**, zero failures), locked release build: **PASS**. Checks/build subprocesses each exited **0**; enclosing verification exited **0**. Separate streams, exact child commands and timestamps are in independent `verification/`. Offline mode was inherited; no install/lock update.
- All **9** PowerShell scripts parsed without syntax errors, without native scenario execution. Evidence: `artifacts/review-sol-m2a-iteration-1/powershell-parse.json`.
- Independent release SHA-256: `E288FF35A3416C9A8AA0A0912C870EF152CDD1F0BA98197EC7FCF3AD7A19848A`. This separate newly linked output is **not** the frozen runtime binary; no byte-reproducibility/runtime claim for the rebuild. Untouched `target/release/roqa.exe` remains `78A4C04E562977EE53482E11FA30ACF927AA828184BFD69641FFAA549665224F`, matching frozen runtime records.

### Author evidence inspected, not independently rerun

- Frozen verification has exit-zero check/build records and seven-test output, consistent with my offline checks.
- Catalog JSON contains **61** successful checks; **501** title changes produced **4** targeted batches/**4** inspections/**0** full scans. All four lifecycle samples are **11,116,544 private bytes / 248 handles / 15 USER / 0 GDI**. Timed reconciliation occurred once; overall counters are 2 full scans/47 targeted batches. Publication during the sleeping storm and **169.1823 ms** quit were recorded. Final zero-hooks/stopped diagnostics follows successful explicit cleanup/join. Credible observations for that harness, not saturation/overflow/failure-injection proof.
- Both shell JSONs have **305** checks and resource plateaus. Inspected actual dock-hover/notch-hover PNGs showing scoped M1 capsules; no independent recapture/pixel assertion run. JSONs have no PowerShell host PID/session identity, so the two full runs sharing one host is author-reported, not independently reconstructable from those JSONs alone.
- Idle: **30.0114 seconds**, **15.625 ms** endpoint CPU delta, private bytes **11,005,952 to 11,337,728**, same frozen binary hash. This quantized single author observation is not a product budget, zero-CPU assertion, DWM/GPU/power attribution or competitor comparison. No new performance measurement by me.

### Carried M1 minors

- **M1-SOL-R2-MIN-1, transcript omission:** resolved in reviewed tooling. `scripts/verify.ps1` drains separate redirected streams, records commands/exits and fails on nonzero exit/timeout. My invocation produced retained native stdout/stderr and exit records through that exact script.
- **M1-OPUS-R2-MIN-2, static fixture reuse:** state resets before startup, the fixture thread is retained/joined, and its class unregisters on exit (`tests/NativeProbe.cs:81`, `tests/NativeProbe.cs:103`, `tests/NativeProbe.cs:122`). Catalog smoke repeatedly uses the loaded type across 15 further lifetimes, supporting the successful-lifetime fix. Shell uses a known nonzero foreground baseline without demanding background-test focus rights (`scripts/smoke.ps1:98`). Original normal-lifetime reuse is structurally addressed; I did not rerun native reuse or certify exceptional fixture-startup paths. Full-shell same-host provenance is qualified above.

## Limits and acceptance boundary

No native scenario was run by this reviewer; Opus owns the exclusive native slot. No source fixes, candidate edits, frozen evidence replacements, commits, installations, global settings/PATH changes or registry changes. Only this report and ignored independent outputs were written.

Not established here or by the frozen packet: saturated timer starvation, forced ingress/pending overflow, dropped notifications, native enumeration/worker/API failure injection, packaged/shared-host/elevated/inaccessible runtime matrix, hung-source shutdown, physical DPI changes, complete UI/input/accessibility, or system-wide performance attribution. Reducer tests do not constitute actual native failure/source-cache branch injection. Gaps are not fabricated passes; unrelated future gates are not grounds for rejecting this foundation. The scheduling finding itself affects bounded repair/live publication.

M2b still owes real icons, stable packaged identity, activation with target revalidation, keyboard/UIA/Narrator, actual Arabic/RTL rendering, system text scaling and an IME-capable editable route. UTF-16 metadata length is not rendering/IME evidence. Full M2 requires cumulative Sol + Opus acceptance and explicit stack go/no-go before M3; nothing here waives those gates.

**Final disposition: REVISE M2a foundation on the exact 61-input manifest above for M2A-SOL-MAJ-1. M2A-SOL-MIN-1 is optional/nonblocking.** Freeze corrections as a new candidate and obtain fresh independent review tasks; this report approves no unseen changes.
