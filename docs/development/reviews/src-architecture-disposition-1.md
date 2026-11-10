# ROQA src architecture review 1: completed disposition

Author: Sol 6.1 Medium architecture worker. Date: 2026-11-11 Africa/Cairo (2026-11-10 UTC). Target: src/ at base 8eb909b7b5825fcba2d4f8ff8c81e5536852ffa0 and the subsequent bounded icon-cache candidate. This successor closes the workflow without altering either frozen review or candidate inputs. It supersedes pending-state statements in the historical native report; it does not confer milestone acceptance.

Skill: C:/Users/Ahmed/.claude/skills/gstack/plan-eng-review/SKILL.md, updated first from 1.97.0.0 to 1.99.0.0, source f0cd9f8c4b453c1eadd714e4149174ee01179eae. Exact original assessment: reviews/src-architecture-review-1.md. Exact original 37-input manifest SHA-256 CB5467952366F4BD5E616832DE6D1874BEA1C9ADF1B18A5414FED0C2673A06EC. Preserve this historical pre-fix manifest; it is not the current patch manifest.

## Scope Challenge

User explicitly selected src/, existing docs (B), then authorized independent Opus assessment, justified bounded fixes, local commits/integration and next bounded M4 feature delegation. Scope accepted as-is. The root main and origin/main agreed at 8eb909b when review started; no branch diff existed then. No rewrite, new architecture layer or unrelated media feature was accepted.

## Architecture review

The 20-file arrangement and resource ownership remain appropriate: Desktop/Surface own UI state, Canvas owns graphics resources, Catalog owns source worker and generation, IconService owns icon worker/cache, and Media owns session/subscription/async slots. File length alone does not justify extraction. No worker framework, process isolation, plugin abstraction or additional cache. Original receipts and flow diagrams are in the frozen report.

AR1-A1's whole-cache-reset churn mechanism is confirmed but needs more than 256 simultaneous distinct apps; Opus revised severity to minor/P3. Its visible-prefix remedy was deferred because it does not solve closed-app retention or admission capacity. Opus identified a more ordinary footprint mechanism: cached closed apps were never evicted. AR1-OPUS-1 (minor/P2) is resolved on the candidate by admission plus retention. No measured CPU/memory claim follows.

## Code quality review

AR1-OPUS-2 (minor/P3), old-size completions retiring new pending work, is resolved by validating size before pending removal and requiring pending membership. AR1-Q1 remains a carry-forward contract: slice 3/4 must extend media redraw comparison whenever rendered fields grow. Preserve explicit ownership, COM release order and FFI boundaries. No unsafe code, timer, worker, wake source or dependency was introduced.

Fresh clean-code-guard by Claude Opus 5.5 High approved with nits: reviews/icon-cache-guard-1.md, report SHA-256 DFA75A9921E83103C5023BED5648130C222A390FFF2EE633787A60FF5D9658A2. Author did not approve own changes. This reviewer did not edit implementation.

## Test review

AR1-T1 is resolved for the accepted remedy: channel-owner tests enforce convergence beyond capacity, cached/pending eviction and rejection of late evicted completion, old-size result ordering, and queue-full retry. Existing alpha/source tests remain. No shell extraction is mocked as a successful native run.

Flow: synthetic demand -> bounded request channel -> test completion -> owner validation -> cached result or rejection. Tests assert no further request after saturation and no current pending cancellation by old-size output. Host eviction call is source-inspected; no GUI assertion is claimed. No LLM/eval scope. QA artifact: C:/Users/Ahmed/.gstack/projects/roqa/ahmed-t3-improve-codebase-architecture-eng-review-test-plan-20261111-001000.md. Tasks artifact is in the same gstack project state; generated state excluded from commits.

## Performance review

Bound is cache + pending <= 256, with disjoint sets. Closed-app state is pruned on changed dock items; work is bounded by existing catalog/cache limits. Existing blocking idle wait and channel-driven completion remain. No new polling. AR1-A2/AR1-OPUS-3 synchronous platform latency and source shutdown join remain documented unknowns pending native evidence.

## Decision ledger

- D1/D2: implement Opus's smaller cache admission/retention and DPI-order fixes. User's explicit direction authorizes justified bounded fixes after independent assessment. Implemented by Sol; independently approved on exact inputs by a fresh Opus review.
- D3: four owner regression tests accepted under the same explicit bounded-fix/check authorization. Implemented and passed.
- D4: defer visible-prefix demand, platform isolation, and render-projection refactor. No speculative work before features.
- D5: ICG1-1 rare queue-full retry gap is deferred P3: after every queued app closes, all their completions may be rejected and a new app may retain a placeholder until the next dock event. It has no leak or extra wake; it is non-blocking. Revisit with real evidence or a later icon change. Do not add polling.
- D6: ICG1-2 missing shortcut placeholder comment and ICG1-3 test naming/spacing nits deferred to avoid changing frozen approved source for cosmetic churn. No fixes claimed.
- D7: user authorizes relevant completed-work commit then single-writer integration; user's root .gitignore remains untouched/excluded; no push. Next bounded M4 slice 3 implementation delegated after integration. GUI requires fresh desktop-free confirmation per run; keyboard/accessibility remains DEFERRED/NOT RUN.
- Approval readiness: PASS for D1-D7, checked against user's explicit review/fix/commit/feature sequence. Deferred work remains unresolved below; authorization does not waive evidence gates.

## What already exists

Reuse existing IconService, dock refresh, local pinned scripts, media/session owner and dock UIA patterns. Candidate details: icon-cache-candidate-1.md. Exact current 39-input manifest: reviews/icon-cache-candidate-1.inputs.sha256, SHA-256 1B4FADE410BE3B8261E6DAB219525F2AC01788F23AE21AB4BC8C9EA255FF207D. Reviewer verified all entries at start/end with unchanged HEAD. Original outside assessment: reviews/src-architecture-opus-1.md, SHA-256 07AD1E6FBDF4E15B6C7F90E4F8BFA8290A79FEDC061E7D50903C6D39AD7EE63D.

Outside architecture task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Ad7b27b05-abe9-4d27-97e4-0c6708ac6651%3Adelegate-task%3A36aa3db2-274e-4c25-a485-ab73ea5bf296>.

Fresh implementation guard task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Ad7b27b05-abe9-4d27-97e4-0c6708ac6651%3Adelegate-task%3A6a20210d-8a80-4b29-a84c-8a0a4ea40936>.

## NOT in scope

Visible-prefix optimization, LRU/byte budgets/per-DPI caches, shared worker framework, media process/thread rewrite, progress/seek/art/volume/visualizer and native tests. They require separate justified scope or the planned later slices. Existing taskbar, keyboard/UIA, text scaling/RTL/IME and always-animate behavior are preserved.

## Failure modes

Full queue leaves a request retryable; test enforces that state. Capacity refusal keeps the neutral tile and does not spin; convergence test. Old-size completion cannot retire new pending state; ordering test. Evicted completion cannot restore a closed icon; retention test. Rare absence of retry trigger is explicit ICG1-1. Native shell/platform hangs and real resource measurements remain unknown; no newly claimed silent critical gap or runtime pass.

## Implementation Tasks

- [x] T1 (P2; estimated human ~1h / agent ~15min): implement bounded icon bookkeeping and four regressions; surfaced by AR1-OPUS-1/2 and AR1-T1. Files: src/icons.rs, src/host.rs. Fresh independent review approved. Pinned verify required again immediately before commit.
- No new architecture extraction tasks. Sequential implementation, no parallelization opportunity. Next controls slice has its own author, frozen inputs and fresh reviews.

## Completion summary

Scope Challenge: accepted as-is. Original reality rows: 7/7 applicable, one unverified native premise; unchanged source receipts remain historical. Architecture: 2 native findings, priority/remedy revised by Opus. Quality: 1 carry-forward contract. Tests: diagram and gap captured; four accepted regressions now pass. Performance: bounded mechanism fixed, no measured gain asserted. NOT in scope, existing solutions and failure modes recorded. No TODOS.md created. No new critical gap. Six explicitly deferred groups below; no unanswered choice prevents this bounded local commit. Outside voice completed via claudeAgent/claude-opus-5-5 high; implementation guard was a separate fresh task. No parallel lane; Lake Score N/A. Author frozen verify passed checks/release, 59 tests; reviewer independently reran checks, 59 passed. Final pre-commit verify and root integration evidence are recorded separately in the ledger. Native GUI/performance, keyboard/accessibility and full milestone council acceptance remain DEFERRED/NOT RUN.

## Suppressed findings

No cosmetic rewrite or architecture scoring. No invented performance improvement. Opus challenged the original proposed remedy and this disposition accepts the smaller supported fix.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|---|---|---|---|---|---|
| CEO Review | Not requested | Scope and strategy | 0 | NOT RUN | No new CEO credit |
| Outside Review | User-requested Opus 5.5 High | Challenge frozen src assessment | 1 | COMPLETED | Revised A1 severity/remedy; bounded fix recommended |
| Eng Review | plan-eng-review / src | Architecture, quality, tests, performance | 2 records, 1 review cycle | ISSUES OPEN, bounded patch approved | Two fixes and test gap resolved; deferred limitations remain |
| Design Review | Not requested in this review | Existing accepted direction | 0 | NOT RUN | No design change |
| DX Review | Not requested | Existing pinned tooling | 0 | NOT RUN | No new DX credit |
| Independent guard | Fresh Opus 5.5 High / clean-code-guard | Non-author implementation review | 1 | APPROVE WITH NITS | 1 minor P3, 2 nits; no blocker |

**OUTSIDE COVERAGE:** claudeAgent/claude-opus-5-5 high completed original architecture assessment and separate fresh implementation guard; each verified exact start/end inputs.
**CROSS-MODEL:** Sol 6.1 Medium original assessment plus Claude Opus 5.5 High challenge overlap on capacity mechanism, platform unknowns and redraw contract; Opus corrected severity and remedy. This is not the required Sol High + Opus High milestone council.
**VERDICT:** Bounded patch independently approved for authorized local commit after final verify. Broader architecture record remains ISSUES OPEN; eng review required for later production changes. No milestone/runtime acceptance.
**UNRESOLVED DECISIONS:**
- AR1-A1 visible-prefix optimization deferred; excess-capacity identities retain placeholders.
- AR1-A2/AR1-OPUS-3 platform latency/shutdown uncertainty deferred pending native evidence.
- AR1-Q1 render-invalidation contract carried into M4 slices 3/4.
- ICG1-1 rare rejected-completion retry gap deferred P3.
- ICG1-2 and ICG1-3 comment/test style nits deferred.
- Native performance, GUI, keyboard/accessibility and milestone council evidence remain deferred/not run; no acceptance credit.
