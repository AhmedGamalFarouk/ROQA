# Icon cache candidate 1

Author: Sol 6.1 Medium architecture worker. Date: 2026-11-11 (Africa/Cairo). Base: 8eb909b7b5825fcba2d4f8ff8c81e5536852ffa0. Bounded bug fix following the independent architecture assessment; not milestone acceptance.

## Decision ledger

- D1 / AR1-OPUS-1: accept admission bound of cached plus pending <= 256; remove whole-cache reset. Evict cached and pending identities absent from the changed dock list. Author implemented in src/icons.rs and src/host.rs.
- D2 / AR1-OPUS-2: accept size validation before pending removal, require actual pending membership before caching. Prevent late old-size results from retiring current-size work.
- D3 / AR1-T1: four owner/channel regression tests cover capacity convergence, cached and pending eviction including late completion rejection, stale-size ordering, and queue-full retry. No shell extraction in these tests.
- Authorization: user's explicit sequence authorizes justified bounded fixes and appropriate checks after Opus assessment. These remedies follow that completed assessment; no speculative rewrite.
- AR1-A1: mechanism confirmed, severity revised to P3 for >256 simultaneous distinct apps. Visible-prefix remedy deferred per Opus: it does not solve stale retention or the capacity bound. AR1-A2/OPUS-3: native call and shutdown latency remain unknown; no threading rewrite. AR1-Q1 carries into M4 slice 3/4 redraw contract.
- Approval readiness: PASS for D1-D3 authorization only. Independent implementation review pending; author cannot approve own work.

## Changes and existing ownership

Existing IconService owns channels, shell STA worker, cache and pending set. Request -> bounded channel -> existing worker -> bounded completion channel -> owner validation -> cache -> repaint. The bound applies at admission; completion never clears live entries to make room. Catalog changes prune closed identities before new demand. No new thread, wake, timer, polling, dependency, cache limit or global hook.

A closed/reopened same identity at the same size may receive an earlier request's result; identity/size are still valid, and subsequent duplicate completion is ignored. No new generation system is warranted by this bounded cache fix. Eviction predicates scan the existing bounded app list only when dock items change. Existing input, UIA and animation behavior is preserved.

## Verification

Pinned scripts/verify.ps1 -EvidenceDirectory artifacts/verification/icon-cache-1-frozen: PASS, checks and release build exit 0. Checks include rustfmt, clippy -D warnings and 59 unit tests (four new); result logs retained as generated local evidence, excluded from commits. Earlier icon-cache-1 run was in progress during author edits and is superseded by this completed frozen-input run.

No native GUI run, real-player interaction, idle CPU/wake/memory measurement, keyboard/accessibility run or runtime council acceptance. All remain DEFERRED/NOT RUN. Always animate as previously directed, including effects-off. Native taskbar remains available.

## Implementation tasks and next gate

- T1 (P2; estimated human ~1h / agent ~15min): bounded icon bookkeeping fix and four regressions, implemented; independent clean-code-guard pending. Files: src/icons.rs, src/host.rs. Verify with pinned scripts/verify.ps1.
- Sequential implementation, no parallelization opportunity. Fresh reviewer may write only its own report and cannot implement fixes. Exact input manifest is icon-cache-candidate-1.inputs.sha256; original architecture freeze remains immutable historical evidence.
- After review/fix gate, verify immediately before authorized commit, integrate via one delegated root writer preserving/excluding user's root .gitignore, then delegate M4 slice 3 controls to Opus 5.5 High. No push. Next feature has its own review/check gate.
