# M2a candidate 2: deadline scheduling and lost-wake recovery

Author: `/root` (implementation, tests and this packet). Date: 2026-11-09.
Status: author verification complete; independent rechecks required. This is round 1 after the initial review. This candidate accepts no milestone by itself.

## Scope and baseline

M1 is accepted. M2a candidate 1 received Opus ACCEPT and Sol REVISE on manifest `C493F3615CBE4CEE643E3264D67C50961B09E44CD0C48A96E418A0A5BDAFBBAC`. Sol identified an actual forward-progress hole: low-priority WM_TIMER scheduling can be starved by sustained higher-priority messages. No previous runtime stall was claimed. Both initial reports remain authoritative for their exact input; this revised source requires both fresh acceptances.

Visible UI remains the M1 capsules. M2b (icons, stable packaged identity, activation, keyboard, UIA/Narrator, Arabic/RTL rendering, system text scaling, IME) and the cumulative stack go/no-go are still required before M3. No feature/gate has been waived.

## Changes and finding mapping

- **M2A-SOL-MAJ-1, fix-submitted:** `catalog_timing.rs` owns monotonic settle/repair/retry deadlines. `host::event_loop` services elapsed catalog work before every bounded loop pass and limits MsgWaitForMultipleObjectsEx to the next deadline, rounded up to milliseconds. It no longer waits infinitely with catalog deadlines outstanding. Catalog WM_TIMER registrations/dispatch are removed. The 50-ms interval never slides when more events arrive; there is still one in-flight worker request, bounded channels, and a 30-second repair deadline. Follow-ups preserve the same 50-ms batching.
- **M2A-OPUS-MIN-1, fix-submitted:** each outer-loop service drains the callback ring and clears NOTIFIED without relying on DIRTY delivery; both normal messages and deadline service share this drain. Completion is also collected on every service pass without requiring COMPLETE delivery. If a wake is lost while otherwise idle, the finite repair deadline recovers it within at most the next 30-second interval; this is bounded recovery, not a promise of normal interactive latency following lost notifications. Callback delivery and draining remain on the registering UI thread with no message pumping inside the drain.
- **M2A-OPUS-MIN-2, fix-submitted:** incomplete results retain the last complete model and schedule full retry after 1, 2, 4, 8, 16 then at most 30 seconds. Event invalidations cannot bypass the retry floor. Complete results reset backoff. Repair intent coalesces with pending/in-flight work and respects that floor.
- **M2A-OPUS-MIN-4, partial coverage submitted:** three clock tests plus one real Catalog service/channel test cover deadline progress without WM_TIMER, non-sliding batching, retries under new events, lost DIRTY latch, capacity overflow -> full request, lost COMPLETE -> publication, follow-up delay, incomplete -> timed full retry, periodic full repair and unexpected disconnected result channel. No native catalog/hooks are started by unit tests; scalar ingress is injected in the single test that owns those statics. No claim of complete source-cache failure coverage.
- **M2A-OPUS-MIN-3, deferred/nonblocking:** explicit diagnostics remain fail-fast if a requested output cannot be written. Default resident launches do not write diagnostics. Transient antivirus/indexer locks can still fail opt-in evidence runs. Changed scheduling does not swallow errors or manufacture evidence.
- **M2A-SOL-MIN-1, deferred/nonblocking:** constructor decomposition is unchanged to keep this correction focused on forward progress. Existing RAII cleanup and ownership remain.

Changed production files: new `src/catalog_timing.rs`; `src/window_catalog.rs`, `src/host.rs`, `src/main.rs`. Source scanning/filtering/grouping, shell rendering, IPC, dependencies/lock/toolchain and normative PLAN are unchanged. Harness changes: `tests/NativeProbe.cs`, `scripts/catalog-smoke.ps1`; README reflects deadline behavior. Exact textual diff from the original frozen zip is retained in `artifacts/m2a-candidate-2/candidate-diff.txt`.

## Author verification and evidence

All final evidence is in `artifacts/m2a-candidate-2/`. Frozen release binary SHA-256: `779D62B0567FA049CA80BAD9B771542B92849E0920F7592373230C8E94EF2988`.

- `verification/`: fmt, locked all-target Clippy with warnings denied, **11 tests**, locked release build PASS; subprocesses exit 0 with stdout/stderr and commands retained.
- `catalog/`: native **PASS 67**. Existing real fixture lifecycle behavior preserved. 501 title changes -> **5 targeted batches, 5 inspections, 0 full enumerations**, 308.1411 ms observed convergence. Four resource samples identical: **11,206,656 private bytes / 248 handles / 14 USER / 0 GDI**. Quit under ongoing title events **183.1402 ms**, explicit zero-live-hook/stopped record after worker join.
- New sustained queue scenario: one owned producer posts harmless WM_APP+50 thread messages for 36 seconds at maximum rate. **3,495,344 posts succeeded, 70,655 were rejected at queue capacity**. Catalog published changed fixture metadata and completed periodic repair while the producer was still alive. This proves progress under this observed capacity-reaching workload; it does not claim the queue was continuously nonempty or that Windows delivered every WinEvent. Producer is joined before the test proceeds and in cleanup. No unrelated app is targeted.
- `shell/` and `shell-reentry/`: **PASS 305 each**, called sequentially from one PowerShell process, with actual pixel assertions/captures, focus and geometry, graceful exits, failure probes and in-process cycles. Same-host provenance is author-reported; JSON format was not changed in this correction. No image/UI behavior changed.
- `idle/idle.json`: release, 5-second settle and **30.0163 seconds** measured. CPU delta **0 ms at endpoint-counter resolution**, private bytes **10,997,760 -> 11,198,464**, handles **231 -> 234**. This is an observation, not zero resource use, a budget result or a competitor comparison; DWM/GPU/power are unattributed.
- Microsoft primary contracts for MsgWaitForMultipleObjectsEx, WM_TIMER and PostThreadMessageW fetched successfully (HTTP200); URL/title/bounded excerpts in `api-contracts.json`. These confirm finite-timeout waits, timer priority and thread-message modal-loop limits. No newer third-party APIs/dependencies introduced.
- All catalog/shell/idle identities match the binary SHA above. No ROQA process intentionally left running; no global configuration, taskbar, DPI setting, startup or registry mutation.

## Remaining limits

No physical display scale/topology change, packaged/elevated/hung-source matrix, real modal-loop message loss, native lost-event/forced-overflow injection, or source native API fault injection was performed. Deterministic scalar/channel injection is explicitly distinguished from those native cases. The capacity test does not retroactively reproduce the original timer defect. Constructor decomposition and diagnostic transient-lock policy are deferred minors. Fullscreen handling remains M3. All M2b UI/accessibility/text/input and full-M2 go/no-go requirements remain open.

## Independent recheck packet

Use `reviews/m2a-iteration-2.manifest` for current inputs; original manifests/reports/packet are baseline context only and their nested old source hashes are superseded. Mutable progress, new review reports and reviewer-owned evidence are excluded. Preserve source/evidence in `artifacts/review-snapshots/m2a-iteration-2.zip`. No candidate mutation while either review runs.

Fresh Sol High: prior findings, exact diff/regressions and independent clean-code-guard. Read guard skill and relevant references. Offline scratch build/check allowed; **no GUI launches** while Opus owns native slot. Fresh Opus High: prior findings, changed scheduler/runtime paths; exclusive native slot for unmodified catalog/shell scripts and independent evidence outside frozen paths. No system changes, unrelated-app termination or frozen-output overwrite. Rechecks are scoped to prior findings/diff/regressions; new unchanged-content findings must be material and evidenced per PLAN. Record stable IDs, exact manifest SHA, checks/limits and ACCEPT/REVISE. Neither reviewer authored any candidate change.
