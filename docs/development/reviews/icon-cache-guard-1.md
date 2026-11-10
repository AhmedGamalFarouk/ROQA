# Icon cache candidate 1: independent clean-code-guard review 1

Reviewer: Claude Opus 5.5 High, fresh read-only review subagent. Not the author (the author is Sol 6.1 Medium). I edited no source, doc or ledger. This file is my only write. Date: 2026-11-11.

## Exact inputs

- HEAD `8eb909b7b5825fcba2d4f8ff8c81e5536852ffa0`, unchanged at the start and end of the review.
- Manifest `docs/development/reviews/icon-cache-candidate-1.inputs.sha256`, raw-byte SHA-256 `1B4FADE410BE3B8261E6DAB219525F2AC01788F23AE21AB4BC8C9EA255FF207D`, 39 entries. `sha256sum -c` result: all 39 OK at the start and again at the end.
- Reviewed diff: `git diff 8eb909b -- src/icons.rs src/host.rs` (+120/−7). Manifest hashes: `src/icons.rs` `ec7ce43b…a7909562`, `src/host.rs` `5fd2b422…dd597a`.
- Callers read: `Desktop::refresh_dock`, `dock_icons`, `place` (dock create), `refresh_fullscreen`, `animate`, `update`. Also read: `graphics.rs` icon borrow (`Option<&IconPixels>`; it does not retain pixels).
- The old architecture manifest (`src-architecture-review-1.inputs.sha256`) describes the pre-fix source on purpose. It is historical evidence, not a stale candidate.

## Checks

- I re-ran pinned `scripts/check.ps1` on these inputs: exit 0. Results: fmt, clippy `-D warnings`, 59 passed, 0 failed. All four new `icons::tests` passed. PowerShell 5.1 wrapped cargo's stderr "Finished" line as `NativeCommandError`; this is cosmetic.
- Author evidence `artifacts/verification/icon-cache-1-frozen/verification.json`: checks and release build exit 0, 59 passed. I read it but did not rebuild the release.
- NOT RUN: native GUI (no per-run desktop-free authorization), idle CPU, wakes, memory, real shell extraction, keyboard/accessibility (DEFERRED by the user). Behavior in those areas is preserved by construction only.

## Correctness walk

- **Bound invariant.** `cache ∩ pending = ∅`, and `cache + pending <= CACHE_LIMIT` always holds:
  - `icon()` admits only an uncached, non-pending identity, and only below the bound.
  - `collect()` moves an entry pending → cache only when `pending.remove` is true.
  - `retain` and the size change only shrink the sets.
  - A duplicate or unmatched result can no longer add an uncounted entry.
- **Capacity convergence.** Past 256 live distinct apps, admission stops, nothing is enqueued, and `collect` returns false. The arrival → clear → re-request churn of AR1-OPUS-1 is gone. The extra apps keep the neutral tile; this is the accepted AR1-A1 P3 limitation.
- **Eviction.** `retain` runs only when the dock items change, before `self.dock_items = items`, against the new list. Title-only generations skip it. The O(n·m) scan is bounded and runs only on item changes. Graphics only borrows the pixels, so the evicted `Arc`s are actually freed.
- **Late DPI completion.** The size is checked before pending is removed, so an old-size result can no longer retire a current-size pending entry (AR1-OPUS-2). Up to `QUEUE` already-queued old-size extractions still run; this was informational in Opus and is unchanged.
- **Late evicted identity.** An evicted identity is no longer pending, so its late result is dropped and does not repaint.
- **Reopen.** If an app closes and reopens while its old request is still queued, the next same-size result is cached (valid pixels for that key) and the duplicate result is dropped. At most `QUEUE` duplicate extractions can occur. The packet documents this, and it is acceptable.
- **Queue-full retry.** A failed `try_send` leaves the identity un-pending, so a later `icon()` retries it. Test 4 covers this. ICG1-1 covers the one case where no retry trigger arrives.
- **Ownership, COM and lifecycle.**
  - Unchanged: the worker STA init/uninit, the 1 s detach in `Drop`, the bounded channels, and `extract` release paths.
  - The tests build the service with `worker: None`, which `Drop` already handles.
  - No new thread, timer, poll, wake, dependency, hook or unsafe code.
- **Idle and wakes.** No new wake source. `ICON_READY` is still posted once per completion, as before. Rejected results now skip the repaint, so there are fewer paints, not more.

## Findings

### ICG1-1 [MINOR / P3, non-blocking] Rejected completions no longer guarantee a queue-full retry

- **Evidence.**
  - `src/icons.rs` `collect` returns true only for accepted results (`size == self.size && self.pending.remove(&identity)`).
  - `src/host.rs` `refresh_dock` returns early when the generation is unchanged and `!icons_arrived`.
  - The Opus review relied on this guarantee: "A full request queue implies a result is coming, and that result triggers the retry". After the change, that result may be rejected.
  - The new trigger is pending eviction in `retain`. The DPI path had the same gap before this patch.
- **Failure path.**
  1. The request queue holds 64 requests.
  2. A dock change evicts every one of those queued identities, and a new app's `try_send` fails.
  3. Every remaining completion is rejected, so `collect` is false and no `dock_icons` pass follows.
  4. The new app shows the neutral tile until the next item change, dock hover animation, fullscreen change or dock creation.
- **Impact.** No leak, wake, churn or crash. The tile is cosmetic and heals itself. Reaching it needs at least 64 in-flight extractions that all close at once while a new app opens.
- **Minimal remedy (author's choice, not required for commit).** Option 1: make `collect` report true when any result was drained. This costs one extra dock repaint per rejected batch, with no new wake. Option 2: record the gap in a one-line `shortcut:` comment.

### ICG1-2 [NIT] The cap comment dropped the known-limit wording

- **Evidence.** `src/icons.rs:33` reads "Bound retained pixels and admitted work together; closed apps are evicted on catalog changes." It no longer states the accepted limitation: past `CACHE_LIMIT` live distinct apps, the extras keep the neutral tile.
- **Remedy.** Restore the project's `shortcut:` form, as in Opus step 4. For example: `// shortcut: past CACHE_LIMIT live distinct apps the extras keep the neutral tile; add LRU if real sessions exceed it.`

### ICG1-3 [NIT] Test readability

- **Evidence.** In `demand_beyond_capacity_settles_without_resetting_cache`, the loop variable `for request in incoming.try_iter()` shadows the helper `fn request`. Separately, `full_request_queue_leaves_identity_retryable` has no blank line before the next `#[test]`.
- **Remedy.** Optional: rename the loop variable to `queued` and add the blank line. Cosmetic only; it does not block.

## Prior findings on these exact inputs

- **AR1-OPUS-1: RESOLVED.** Admission is bounded (`cache + pending >= CACHE_LIMIT`), the whole-cache clear is removed, and closed apps are evicted on item changes. Test 1 would fail on the old clear-and-re-request behavior.
- **AR1-OPUS-2: RESOLVED.** The size is checked before pending is removed, and pending membership is required before caching. Test 3 covers this.
- **AR1-T1: RESOLVED** for the bookkeeping scope.
  - The four owner-level, channel-only tests match the Opus assertions. Tests 2 and 3 go further than Opus required: test 2 also checks pending eviction and late-result rejection, and test 3 checks pending preservation.
  - The three-line host `retain` call has no test; host tests would need native surfaces. This is an acceptable gap.
- **AR1-A1** stays an accepted P3 limitation. **AR1-A2, AR1-OPUS-3 and AR1-Q1** carry forward unchanged; this patch does not touch them.

## clean-code-guard (review mode)

- **Section A (naming and functions): clean.** `retain` mirrors `HashMap::retain`, with a one-line contract. The functions are small and have at most 3 parameters.
- **Section B (comments and formatting): ICG1-2 and ICG1-3 nits.** The comments that were kept explain why.
- **Section C (SOLID): clean.** `IconService` remains the single owner of the cache, pending set and worker. The host supplies only the predicate.
- **Section D (DRY/KISS/YAGNI): clean.** No new type, generation system, LRU or config. The diff follows the smallest fix proposed by Opus. Evicting pending as well as cache is justified by the admission bound.
- **Section E (AI failure modes): clean.** No swallowed errors, no fixture returns and no speculative options. The tests drive real channels, with no shell call. No weakened test.

Outcome: **1 MINOR (P3, non-blocking), 2 NIT. No blocking or critical finding.**

## Disposition

**APPROVE WITH NITS.** The bounded patch is safe for the authorized local commit, provided the pinned `scripts/verify.ps1` runs immediately before the commit. Fixing ICG1-1, ICG1-2 or ICG1-3 is optional. If the author applies a fix, this reviewer cannot approve it; any fix needs a fresh non-author recheck under its own manifest.

**Limitations.** This is not runtime, performance, milestone or council acceptance. Still NOT RUN: native GUI, idle, wake and footprint evidence, real shell latency, and keyboard/accessibility (DEFERRED by the user). The native taskbar remains available. No global state was changed.
