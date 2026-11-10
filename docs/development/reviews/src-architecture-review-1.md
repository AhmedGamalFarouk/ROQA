# ROQA src architecture review 1: native assessment

Author/reviewer: Sol 6.1 Medium, this implementation/architecture thread. This reviewer did not author the source under review. Date: 2026-11-10 UTC. This is an architecture assessment, not milestone acceptance or approval of later edits.

Target: all 20 Rust files in `src/` at `8eb909b7b5825fcba2d4f8ff8c81e5536852ffa0`. The branch and fetched `origin/main` agree. The earlier branch-diff target was empty; the user explicitly changed the target to `src/`. Exact raw-byte inputs are frozen in `src-architecture-review-1.inputs.sha256` beside this report. The manifest includes this report; its own SHA-256 is recorded separately in the ledger/delegation.

Workflow: gstack `plan-eng-review`, installed at `C:/Users/Ahmed/.claude/skills/gstack/plan-eng-review/SKILL.md`, upgraded from 1.97.0.0 to 1.99.0.0, source `f0cd9f8c4b453c1eadd714e4149174ee01179eae`. Session `1-1794345665-7eedbd30`. Existing project context selected by user answer B; no office-hours run. Optional routing setup is outside this review and has not been applied to ROQA.

## Scope Challenge

Scope accepted as-is: inspect 20 existing source files, introduce no feature or new module during review. The original arrangement has 20 files and zero proposed new services. A smaller arrangement would merge distinct platform and pure-state ownership without a demonstrated benefit. Retain the arrangement as the review baseline. The latest explicit user direction authorizes routine reversible implementation choices without repeat permission; this record does not authorize speculative rewrites or waive independent review.

No scope-cut findings. Minimum next action: independently challenge these findings, fix only justified material defects, then move to the already planned M4 slice 3. No new dependency, process, plugin layer, common worker framework, background scheduler, or architectural pattern is proposed.

Existing solutions: `WindowModel` and `ScanSchedule` own catalog state/generation; `CatalogTiming` owns debounce/backoff; `IconService` owns one worker and cache; `Media` owns GSMTC subscriptions and async slots; `Desktop` owns UI state; `Surface::Role` separates dock/notch state; `Canvas` owns composition targets; `layout` shares drawing/input geometry. Reuse these rather than introduce competing owners.

History inspected: source commits through M4 slice 2; the Surface role split, clock gating, fullscreen gating and graphics recovery already exist. No source revert was found. There is no `TODOS.md`. The root user's `.gitignore` is excluded from all work.

Reality receipts:

| Premise | Receipt | Verdict |
|---|---|---|
| Idle uses a blocking message wait | `src/host.rs:1040` | verified by source |
| Static media paint is change-driven | `src/media.rs:102`, `src/host.rs:928` | verified by source; native counts unknown |
| Icon cache is bounded to 256 entries | `src/icons.rs:34`, `src/icons.rs:146` | verified; admission can still churn |
| Window source admits up to 512 records | `src/window_source.rs:33` | verified by source |
| GSMTC getters are harmless for a hung player | `docs/development/m4-media-state-candidate-1.md:112` | UNVERIFIED, not assumed |
| Current native media appearance and footprint passed | `docs/development/m4-media-paint-candidate-1.md` UNVERIFIED section | false; evidence remains pending |

REALITY: premise-table pass verified source mechanisms; unverified=1 docs/development/m4-media-state-candidate-1.md:112
REALITY: already-done pass checked fetched origin/main and source history src/host.rs:115
REALITY: surface-check pass existing wrappers read; no new CLI flags proposed scripts/check.ps1:3
REALITY: binding-decisions pass always animate; taskbar available; native confirmation required docs/design/roqa-visual-spec.md:112
REALITY: numbers pass source-file count measured; capacities from constants; no speedup claim src/icons.rs:34
REALITY: deferred-asks pass no linked issue; GUI and keyboard validation remain deferred PLAN.md:73
REALITY: pr-count pass no PR or push in this task; sequential local commits only PRODUCT.md:16
REALITY: units n/a no timing-unit change proposed src/catalog_timing.rs:66
REALITY: perf-table finding no fresh native measurements; no measured regression or gain asserted docs/development/m4-media-paint-candidate-1.md:79

## 1. Architecture

Keep the stack and ownership model. There is no evidence supporting a framework migration, event bus, trait hierarchy, or division into more services. `host.rs` and `graphics.rs` are large, but file length alone is not a defect. No new public boundary is proposed, so design-twice does not apply. Alternative consolidation of these files would widen the platform/resource context required for each change and is rejected without a concrete reliability or size benefit.

```text
main::run
  +-- control argument -> per-user/session IPC -> resident host
  +-- resident -> Apartment -> WindowClass -> Controls -> Desktop -> Catalog
       +-- Catalog callback: scalar handles -> bounded ring -> coalesced DIRTY
       |     -> debounce / overflow full-repair -> one worker request
       |     -> SourceState -> complete? WindowModel : retain last good state
       +-- GSMTC callbacks -> atomic dirty bits / one-shot completion slots
       |     -> Media::refresh -> session generation / stale-result rejection
       |     -> drawn fields changed? paint visible notches : no paint
       +-- icons -> bounded request queue -> shell worker -> bounded results
       |     -> cache -> dock refresh -> per-monitor composition
       +-- UI message -> host mutation -> shared layout -> input + pixels
       +-- wait -> animation? compositor frame : blocking message/deadline wait
       +-- paint failure -> device lost? bounded rebuild : explicit error exit
       +-- exit -> subscriptions/workers/surfaces -> HWNDs -> COM guards
```

**AR1-A1 [INFORMATIONAL / P2 / minor] (confidence: 9/10): icon admission exceeds drawable demand and can defeat idle convergence.** `src/host.rs:711` iterates `self.dock_items.iter()` to request icons for every catalog group; `src/graphics.rs:273` later draws only `dock::visible_slots(...)`. `src/icons.rs:146` uses `if self.cache.len() >= CACHE_LIMIT { self.cache.clear(); }`. At more than 256 distinct cacheable identities (the source allows up to 512 windows), a stable catalog can cycle through whole-cache invalidations, extraction results and repaint requests. Even below that threshold, offscreen groups consume shell work and cache entries. This is a source-derived reachable mechanism, not a native performance measurement.

Recommendation [Layer 1, reuse existing geometry]: admit requests for the union of actually drawable dock prefixes across monitors, preserving the picker slot and densest-DPI behavior. Check a second boundary too: a drawable set itself can exceed 256 on unusual displays. Make bounded admission/cache policy converge rather than cycling; choose the smallest complete mechanism after independent review. Do not claim the visibility change alone proves convergence at every capacity. Preserve all groups in the model/picker/UIA. No new polling, worker or generic LRU dependency. Test real owner behavior with channels and synthetic identities, without invoking shell extraction.

Disposition: recommended for independent Opus assessment before implementation. The user has authorized justified bounded fixes after that assessment. No source edited here.

**AR1-A2 [INFORMATIONAL / P3 / risk] (confidence: 7/10): synchronous platform calls remain an unproven latency boundary.** `src/media.rs:325` calls `session.GetPlaybackInfo()?`, then `GetTimelineProperties()?` on the host STA. `src/window_source.rs:191` obtains the shell property store; the 250-ms scan guard is between calls, and `src/window_catalog.rs:333` joins the source worker during shutdown. A cooperative scan deadline cannot cancel a platform call already in progress. Whether these calls hang on this machine/player is unknown; the media packet already records this limit. Do not describe the scan guard as a hard wall-clock shutdown guarantee.

Recommendation: preserve this explicit evidence gate. Do not introduce helper processes or a new media-thread architecture without a failing bounded native scenario. A demonstrated problem requires a separate bounded candidate; do not block current feature development on speculative isolation. Disposition: existing deferred evidence, no immediate source change proposed.

Architecture flag screen: no new shallow module, temporal split, forwarding layer, duplicate state owner, or framework extraction proposed. The concrete coupling is demand/cache admission (A1). Broad render-state exposure (Q1 below) is a future drift risk, not an observed broken frame.

## 2. Code quality

**AR1-Q1 [INFORMATIONAL / P3 / minor] (confidence: 8/10): the drawn-media field contract is hand-maintained across state and rendering.** `src/media.rs:105` compares `(title, artist, playing)` while `src/graphics.rs:599`, `:612` and `:621` consume those fields from a full `Track`. The existing test protects today's list; a future renderer could read another field without updating invalidation. This is also the accepted M4 paint NIT-4; it is not a newly discovered current repaint failure.

Recommendation: carry the contract into slice 3's enabled-state work and slice 4's progress work. Prefer an explicit small render projection only if those changes create a demonstrated omission risk; otherwise extend the existing predicate and its behavioral table. Avoid a stand-alone refactor before feature work. Disposition: documented, carry forward with the next relevant slice, no immediate source change.

Shared-code rubric: icon and catalog workers have different COM apartments, cancellation, result budgets and lifetime guarantees. Their similar request loops are insufficient grounds for a shared worker framework. Combining them would add policy switches and couple unrelated failure modes. No extraction proposed; no invented line-savings estimate. `layout`/`dock::visible_slots` are already the reusable contracts needed by A1.

FFI/cleanup: native-window, picker and enumeration callbacks catch unwind; media callbacks only publish bounded state/wakes; composition draws attempt EndDraw after content failure; media cancels requests then unsubscribes; surfaces precede graphics/window destruction. This is static inspection, not a fault-injection pass. Existing text-scale/RTL/keyboard behavior must be preserved; validation remains deferred by user direction.

## 3. Tests

Framework: Rust built-in unit tests, with project-local PowerShell native suites. `scripts/check.ps1` runs fmt, locked all-target clippy and locked tests. Existing baseline ledger records 55 passing tests; this review did not rerun them and does not convert that record into a fresh pass. This isolated worktree has no `.tools` directory; the root has the pinned toolchain. Reuse it locally for later checks without changing global PATH.

```text
CODE / OWNER                                  USER FLOW / EVIDENCE
catalog timing + model                        windows appear/close/focus
 +-- debounce/repair/backoff [unit tests]        -> native catalog suite [prior run only]
 +-- stale/incomplete/reused HWND [unit tests]   -> hung source [current run NOT RUN]
icons                                        app groups overflow dock width
 +-- source order/alpha [unit tests]             -> hidden groups stay in picker [source]
 +-- queue/cache capacity convergence [GAP]      -> stable scene stays idle [NOT MEASURED]
 +-- stale DPI result vs new pending [GAP]       -> DPI transition [current run NOT RUN]
media reducer                                player/session replacement
 +-- generation/text bounds/timeline [tests]     -> real player [slice-1 evidence only]
 +-- drawn-view invalidation [unit test]         -> static paint counts [NOT RUN]
layout + motion + scene                       notch open/close, long/RTL title
 +-- dimensions/hit/overshoot [unit tests]        -> screen/RTL ellipsis [NOT RUN]
host / graphics / native_window               startup, foreground, loss, topology
 +-- wait rounding/rebuild decisions [tests]     -> lifecycle/native suites [NOT RUN here]
picker / dock_uia                             click, UIA invoke, focus return
 +-- target/version checks [source]              -> keyboard/Narrator [DEFERRED by user]
```

This is an ownership/critical-flow map, not an exhaustive line-coverage measurement; no coverage percentage is asserted. All 20 source modules were inventoried and source paths traced; individual Win32 branches require native evidence. No LLM/eval changes apply.

**AR1-T1 [INFORMATIONAL / P2 / minor] (confidence: 9/10): current icon tests do not exercise demand/capacity/result ordering.** `src/icons.rs:286` tests alpha normalization and parsing-name order only. The A1 regression therefore has no guard. Add owner-level tests alongside a justified fix; avoid tests that merely repeat cache internals or introduce production hooks solely for tests. Test budget: idle convergence with a synthetic demand set larger than cache capacity, hidden-group request exclusion, queue-full recovery, stale-size completion, and mixed-DPI visible prefixes. Scope exact assertions to the final independently assessed fix. A stale-result issue should be fixed only if the independent reviewer confirms user-visible or budget impact.

Value: protects=stable icon demand converges to no new work; fails_when=capacity causes repeat extraction; why_new=current tests only alpha/sources; seam=none; no_claim=real shell latency or idle CPU.

Value: protects=offscreen groups do not request icons; fails_when=all catalog groups enter extraction; why_new=no demand test exists; seam=none; no_claim=on-screen native rendering.

Value: protects=old-size result cannot retire current pending work; fails_when=stale completion permits duplicates; why_new=no completion-order test exists; seam=none; no_claim=shell callback timing.

No tests are made obsolete. Native GUI, keyboard/Narrator, accessibility, RTL/IME, DWM/GPU/power and footprint evidence remain DEFERRED/NOT RUN. No new suite, framework or benchmark is proposed by this review.

## 4. Performance

A1 is the immediate performance priority, counted once under Architecture. No other measured performance defect is asserted. `CatalogTiming` has a 30-second repair wake, the clock a minute deadline, fullscreen polling is gated, and compositor pacing runs while motion is live. Thus 'no continuous idle render loop' is supported by mechanism; 'zero idle wakes' would be false. Existing native idle results are inconclusive, and this review supplies no replacement numbers.

Entry caps are not a byte budget: icon bitmap validation permits up to 512x512 BGRA (1 MiB pixel data per maximal result), cache limit 256 and channel limit 64. These are arithmetic upper bounds from `src/icons.rs`, not observed allocations; returned shell image size and normal requested size differ. A demand fix should reduce unnecessary retention/work without raising caps. No global text/layout/bitmap cache is recommended without profiling: short morphs change text sizes, and a process-lifetime cache adds its own footprint and invalidation costs.

Existing graphics composition commits once per Canvas draw and creates resources during paints. Batching/caching might help animation but has no measured benefit here; defer rather than widen a working ownership contract.

## Decision ledger and next actions

R1 target/context: approved by explicit user `review src/` and B (existing docs).
R2 arrangement: retain current 20-file arrangement as baseline; no restructuring. Latest user direction permits routine reversible choices without repeat permission. No feature removed, no new service introduced.
R3 remedy sequence: explicit latest user direction authorizes fresh Opus 5.5 High assessment, then bounded justified implementation and independent clean-code-guard, pinned checks, relevant local commits, safe delegated integration, then start the next bounded M4 feature with Opus. No push; exclude the root user's `.gitignore`.
R4 specific fixes: A1/T1 are recommendations awaiting independent assessment. A2 is existing deferred evidence. Q1 is a carry-forward caution. Approval of the orchestration sequence is not an implementation pass, review acceptance, or runtime waiver.

Approval readiness: PASS for review publication and the user-authorized next review. No implementation candidate is being approved by this report. Specific material dispositions will be recorded after the fresh Opus assessment.

## Implementation Tasks

- T1 (P2, human ~2-4h / agent ~20-45min, estimated): assess and fix visible icon demand/admission convergence, derived from A1/T1. Likely `host.rs`, `icons.rs`, existing geometry helpers/tests. Keep model/picker contents and UIA availability. Verify via channel-based regressions plus project-local verify. Exact design depends on independent findings; do not expand to a worker/cache framework.
- T2 (P3): carry Q1 invalidation contract into M4 controls/progress; part of that feature's necessary regression work, not a separate rewrite.
- No new implementation task from A2: retain the already recorded native hardening gate.

Sequential implementation, no parallelization opportunity. Order: freeze this report -> independent Opus challenge -> justified bounded fix -> fresh non-author clean-code-guard and checks -> local commit/integration -> M4 slice 3 author. No root write until a single integration worker owns it.

## NOT in scope

Full architecture rewrite, new media features during review, caches without measurements, taskbar replacement, global hooks, startup/system settings, player processes, packaging, taskbar/work-area mutation and speculative helper-process isolation. No GUI/native tests without fresh desktop-free confirmation and foreground inspection. Existing missing runtime evidence and council acceptance remain open.

## Failure modes

- Cache churn: bounded in memory by entry count yet may generate repeat work; source-derived, missing regression test (A1/T1), visible as missing/reloading icons and unnecessary repaint activity. Not silent unhandled data loss.
- Shell extraction hangs: UI has placeholders and worker shutdown detaches after one second; not a hard bound on the shell call itself. Preserve existing behavior.
- Hung synchronous getter/source property: evidence unknown; cooperative source budget does not interrupt the call (A2). Existing hardening gate remains open.
- Media session replaced during read: generation guard and cancellation; existing reducer tests, real native replacement not rerun.
- Device removed: bounded retry then error exit; existing pure tests, native recovery not rerun.
- No new production path introduced; zero newly introduced silent, unhandled failure paths. This is not certification that all existing native paths pass.

## Completion summary

Native assessment complete, evidence kind: static source/docs/history inspection only. Scope accepted as-is. Four distinct issues: A1, A2, Q1, T1; A2 and Q1 are existing documented risks, not new regressions. Two test-gap areas are grouped in T1 (demand/capacity and result ordering). No source edits or fresh cargo/native runs. No TODO file created. Lake score N/A (no coverage-scored scope choice). Outside voice is the user-requested fresh Opus 5.5 High task, pending dispatch against this freeze; this native report is not a completed council gate.

## Suppressed findings

No additional low-confidence bug claims retained. File length, duplicate-looking COM guards, per-paint allocations and synchronous platform calls alone do not justify a rewrite or performance claim.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|---|---|---|---|---|---|
| Eng native assessment | plan-eng-review / src | Architecture, quality, tests, performance | 1 | ISSUES OPEN | 4 issues; static evidence only |
| Outside review | User-requested Opus 5.5 High | Independent challenge of this frozen report and source | 0 | PENDING | No clean credit yet |
| Council milestone review | PLAN.md | Milestone acceptance | 0 for this candidate | NOT RUN | Existing deferred runtime gates remain open |

**OUTSIDE COVERAGE:** Opus 5.5 High requested, not yet completed; no substitute CLI invoked.
**VERDICT:** Native assessment ready for independent challenge; no milestone or implementation acceptance.
**UNRESOLVED DECISIONS:**
- A1/T1 exact fix disposition awaits fresh independent Opus assessment; user-authorized bounded remedy sequence follows it.
