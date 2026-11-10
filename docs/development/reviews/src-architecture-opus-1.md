# ROQA src architecture review 1: independent Opus assessment

Reviewer: Claude Opus 5.5 High, fresh delegated read-only reviewer. It did not write the source under review, the Sol report, or any fix. Date: 2026-11-10. This is an architecture assessment only. It is not milestone or council acceptance, and it does not approve any future fix.

## Inputs and staleness

- Base `8eb909b7b5825fcba2d4f8ff8c81e5536852ffa0` (HEAD confirmed).
- Frozen report `docs/development/reviews/src-architecture-review-1.md` and manifest `docs/development/reviews/src-architecture-review-1.inputs.sha256`.
- Manifest file SHA-256 `CB5467952366F4BD5E616832DE6D1874BEA1C9ADF1B18A5414FED0C2673A06EC`: matched at start.
- Raw-byte check `sha256sum -c` at start: 37/37 OK, 0 failed. The end-of-review result is recorded in the delegation return, because this report cannot contain its own hash.
- Also read: `AGENTS.md`, `PLAN.md`/`PRODUCT.md` (manifest inputs), the tail of `docs/development/progress.md` (working-tree modified, not a manifest input), `docs/development/m4-media-preparation.md`, `m4-media-state-candidate-1.md`, `m4-media-paint-candidate-1.md`, and `clean-code-guard` SKILL.md.
- Evidence kind: static source and doc inspection only. No cargo, native, GUI or keyboard run. All runtime evidence is DEFERRED/NOT RUN, and none of it is passed.

## Callers verified

- Icon demand. `Desktop::dock_icons` (`src/host.rs:700-717`) requests an icon for every `self.dock_items` entry at the densest dock DPI. It is called from `place` (`:554`), `animate` while a dock is live (`:599-600`), `refresh_dock` (`:688`) and the full-screen restore (`:831`). `refresh_dock` runs on every loop pass (`:922`). It does work only on a catalog-generation change or when `icons.collect()` reports a cached arrival (`:644-646`).
- Drawing. `Canvas::paint_dock` draws only `dock::visible_slots(self.width, …)` (`src/graphics.rs:273-274`). `Canvas.width` is `bounds.width` (`:181`, `:197`), so the geometry matches the hit test in `Surface::dock_slot` (`src/host.rs:355-358`). The picker and UIA use the model and names, not icons.
- Identity. `src/window_source.rs:161-165`: AUMID, else the executable path, else `Unavailable(pid)`. An `Unavailable` item with no executable has empty sources, so it never enters the cache or pending (`src/icons.rs:122`). Cache keys are therefore distinct apps, not windows.
- Wake. The worker posts `ICON_READY` (`src/icons.rs:89`). The loop wakes and the next `update` collects. No polling.

## Dispositions of the Sol findings

### AR1-A1: PARTIALLY AGREE (mechanism confirmed; severity and remedy revised)

Confirmed by source:
- `src/icons.rs:146-148`: `if self.cache.len() >= CACHE_LIMIT { self.cache.clear(); }`. With more than 256 distinct icon-bearing groups present at once, every arrival past 256 empties the cache. The arrival sets `changed`, `refresh_dock` re-runs `dock_icons`, and every uncached identity is requested again. This repeats without end, so idle never converges. The loop is driven by events, not a spin, but it means continuous shell extraction and repaint.
- Offscreen groups are requested even though they are never drawn (`src/host.rs:711` vs `src/graphics.rs:273`).

Revised:
- **Reachability of the churn is real in source but very unlikely in practice.** It needs more than 256 *distinct apps* with visible windows at the same moment. Windows are grouped by identity, so 512 windows do not mean 512 keys. I rate it minor/P3, not P2.
- **Sol missed a more realistic cost: stale retention.** Nothing evicts identities that leave the dock. The only resets are size change (`:114-118`) and the 256 bound. Every distinct app opened during a long session keeps its bitmap until that point. At 200% scale a 40-DIP icon is 80x80x4 = 25.6 KB, so a full cache is about 6.5 MB of retained pixels for closed apps. That works against the tiny-footprint goal, and it is reachable in ordinary use.
- **Visible-prefix demand alone is neither necessary nor sufficient.** It does not evict closed apps. It only bounds demand by geometry: about 34 slots at 1920 px and 100%, and over 256 only on a work area wider than about 14,400 px at 100%, so the cap is still needed. Hidden groups exist only past roughly 33 groups on a 1080p dock, and each costs one cached extraction. That is a small, bounded saving, so I defer it (see "Deferred").

See AR1-OPUS-1 and AR1-OPUS-2 for the replacement findings and fix.

### AR1-A2: AGREE (informational, no change)

`src/media.rs:325-326` calls `GetPlaybackInfo()?` and then `GetTimelineProperties()?` synchronously on the host STA. `src/window_catalog.rs:333-336` joins the source worker with no deadline. That differs from the icon worker's 1-second detach (`src/icons.rs:164-175`). A shell property-store call that hangs (`src/window_source.rs:191-192`) would therefore hang shutdown. Whether these calls hang on real players or shells is a **documented unknown** (`progress.md`, M4 slice-1 UNVERIFIED list). It is not an observed bug. I agree to keep the evidence gate and not to add a helper thread or process now.

### AR1-Q1: AGREE (carry forward)

`src/media.rs:102-108` compares `(title, artist, playing)`, and `src/graphics.rs:599-621` reads exactly those fields. Today they match. Slice 3 (controls and enabled state) and slice 4 (progress) must extend `redraw` and its test in the same candidate. No standalone refactor is needed.

### AR1-T1: AGREE, re-scoped to the AR1-OPUS-1 fix

Today's icon tests only cover alpha handling and source order (`src/icons.rs:290-320`). The tests to add are listed under "Regression assertions". Mixed-DPI and hidden-group tests apply only if visible-prefix demand is ever adopted.

## New findings

**AR1-OPUS-1 [MINOR / P2] (confidence 9/10): icon cache has no eviction and its overflow policy cannot converge.**
- Evidence: `src/icons.rs:56`. The cache is keyed by every identity ever requested at the current size. `:146-148` clears the whole cache at 256. `:131-133` admits a request whatever the cache occupancy.
- Impact: (a) bitmaps of closed apps stay retained up to 256 entries (footprint); (b) endless re-extraction and repaint when more than 256 distinct groups are present at once (idle convergence).
- This is an observed source mechanism. No native memory or CPU figure is claimed.

**AR1-OPUS-2 [MINOR / P3] (confidence 9/10): a stale-size completion retires the current-size pending entry.**
- Evidence: `src/icons.rs:144`, `self.pending.remove(&identity);`, runs before the `size == self.size` check at `:145`.
- Failure path: the DPI changes, so `icon()` clears pending and requests X at the new size. The old-size result for X then arrives and removes X from pending, but is not cached. Any `dock_icons` call before the new result lands enqueues a duplicate request. The calls that can do this are `animate` during hover, `place`, and the full-screen restore. A stale result with no match that lands later in the same epoch can also add an entry that admission never counted.
- Impact is bounded: at most one duplicate shell extraction and repaint per identity per DPI change, and no lost icon. It shares a code line with the AR1-OPUS-1 fix, so it costs nothing to fix there.
- Not worth fixing: requests already queued at the old size are still extracted after a DPI change, up to 64 (`QUEUE`). DPI changes are rare, and cancelling them would need a generation channel. Informational only.

**AR1-OPUS-3 [INFORMATIONAL / P3] (confidence 8/10): asymmetric worker shutdown bounds.** The icon worker detaches after 1 second. The catalog worker joins with no bound (`src/window_catalog.rs:333`). This is a subset of A2 and stays deferred until native evidence shows a hang. It is recorded so that the "bounded shutdown" wording is not applied to the catalog worker.

No other material findings. Queue-full recovery is correct. A failed `try_send` leaves the identity un-pending. A full request queue implies a result is coming, and that result triggers the retry through `collect` → `refresh_dock`. The results channel blocks only the worker, and the UI drains it every pass. COM ownership is correct: the worker STA is initialized and uninitialized on its own thread, and `extract` releases the factory and the `HBITMAP` on every path.

## Smallest justified fix (single bounded candidate, about 10 production lines)

All changes are in `src/icons.rs`, plus one call in `src/host.rs`. There is no new type, thread, timer, poll, dependency or cap change.

1. `IconService::icon` admission. After the `sources.is_empty() || pending.contains` check, return `None` without enqueueing when `self.cache.len() + self.pending.len() >= CACHE_LIMIT`. Items are requested in dock order, so drawn slots are admitted first in all realistic cases.
2. `IconService::collect`. Delete the whole-cache `clear`. Insert only when `size == self.size && self.pending.remove(&identity)`. This keeps `cache + pending <= CACHE_LIMIT` and fixes AR1-OPUS-2: a stale result no longer retires a current pending entry. A same-size result from an earlier epoch whose identity is pending now has valid pixels, and the later duplicate is dropped.
3. Add `IconService::retain(&mut self, keep: impl Fn(&Identity) -> bool)`, which calls `self.cache.retain(|id, _| keep(id))`. Call it in `Desktop::refresh_dock` only when `changed` (items differ), with `|id| items.iter().any(|item| &item.identity == id)`. Title-only generations skip it. The O(n·m) scan runs only on item changes and is at most 256 × 512 comparisons in the extreme case.
4. Update the shortcut comment at `src/icons.rs:33`. For example: `shortcut: past CACHE_LIMIT distinct live groups the extra ones show the neutral tile; add LRU if real sessions exceed it.`

Preserved: model, picker, UIA names and versions, dock geometry, keyboard, text scale, RTL/IME, the always-animate rule, and taskbar availability. No new wake source is added: idle wakes stay the same, and the churn loop is removed.

### Regression assertions (owner-level, no shell extraction)

The tests build `IconService` directly inside `icons.rs` tests with `worker: None`. The test holds the request receiver and the result sender. No production hook is needed, and `Drop` already handles `worker: None`. These checks are meaningful because each one fails against today's source:

1. **Convergence beyond capacity.** Use `CACHE_LIMIT + 20` synthetic `Executable` identities. Repeatedly call `icon()` for all of them, answer each received request with `(id, size, None)`, and `collect()`, until no new request arrives. Then assert: a further full `icon()` pass enqueues nothing (`try_recv` is Err), `collect()` returns false, and `cache.len() <= CACHE_LIMIT`. Fails today because of the clear.
2. **Eviction.** Cache A and B, then `retain(|id| id == B)`. Assert that the next `icon(A)` enqueues one request, `icon(B)` enqueues none, and `cache.len() == 1`.
3. **Stale-size completion.** Request X at size 40, then call `icon(X, …, 60)`, which re-requests. Deliver `(X, 40, …)`. Assert that `collect()` is false and that a third `icon(X, …, 60)` enqueues nothing (still pending). Deliver `(X, 60, …)`. Assert that `collect()` is true and that X is cached. Fails today because the duplicate is enqueued.
4. **Queue-full retry (guards existing behavior).** Fill the `QUEUE` request channel without receiving. Assert that the next new identity is not pending. After one `recv`, the same call enqueues it.

The checks still required are pinned `scripts/check.ps1` and `verify.ps1` (fmt, clippy, tests). Native idle and footprint stay NOT RUN. The tests prove the bookkeeping only. They make no claim about shell latency, real memory or CPU.

## Deferred (not justified now)

- **Visible-prefix demand (Sol's A1 recommendation).** The saving is small and bounded: about 33 or more groups on 1080p, one extraction each. It also couples host demand to graphics geometry and adds mixed-DPI test needs. Revisit if a real session shows many hidden groups.
- **LRU, per-DPI cache, byte budget, worker framework, helper process or media thread.** No evidence supports any of them.
- **AR1-A2 and AR1-OPUS-3 hardening.** Wait for native hang evidence.

## clean-code-guard (existing src, review mode, scoped to the reviewed paths)

- The ownership is sound. One owner per resource: `IconService` (worker, cache), `Catalog`, `Media`, `Desktop`. COM and WinRT lifetimes are explicit and ordered. FFI callbacks catch unwind or only publish atomics.
- KISS/YAGNI: the existing code does not over-abstract. I agree with Sol that sharing a worker framework between the icon and catalog workers would couple different failure modes.
- The defects are AR1-OPUS-1 and AR1-OPUS-2: bookkeeping placed before validation, and a bound enforced by destroying state rather than by admission. `host.rs` (1225 lines) and `graphics.rs` (1246 lines) are large but cohesive. Length alone is not a finding.
- This guard pass covers the existing source only. It does **not** certify the proposed fix. That fix needs its own fresh non-author clean-code-guard.

## False positives and overreach in the Sol report

- A1's claim that churn "can defeat idle convergence" is true only past 256 simultaneous distinct apps. The report presents it as the main performance priority while it misses stale retention, the more reachable footprint cost.
- A1's recommended remedy (visible-prefix admission) is not the smallest complete fix. It needs a second mechanism anyway, which the report concedes, and it leaves closed-app retention in place.
- T1's "mixed-DPI visible prefixes" test applies only to the deferred remedy.
- No other false positives. A2, Q1 and the receipts table are accurate. Sol correctly states that native evidence is missing.

## Verdict

Proceed with the single bounded icon-cache candidate above (AR1-OPUS-1 and AR1-OPUS-2), then move to M4 slice 3. No reason was found to block feature work. A2 and Q1 carry forward as documented. This is not milestone, council or runtime acceptance.
