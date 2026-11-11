# M4 slice 4 candidate 1: expanded progress and seek

Author: Claude Opus 5.5 High worker thread, launched by the ROQA Chief Thread at the user's request, 2026-11-11. Branch `feat/m4-progress-seek-2`, base **450d46c** (local main, equal to origin/main). No commit or push; the Chief lands it. The author approves nothing; review is by a fresh, independent Claude Opus 5.5 High clean-code-guard task. No Sol was used.

Written under `/ponytail:ponytail` (smallest complete change). No new dependency, Cargo feature, thread, timer, wait handle, event subscription or player abstraction.

## What changed

| File | Change |
|---|---|
| `src/media.rs` | `Track::seekable` (`IsPlaybackPositionEnabled`); `Progress` (whole seconds elapsed/total, filled bar pixels) and its `m:ss` text; `MAX_LENGTH`; `Track::progress`, `next_tick`, `seek_target`, `sought`; `read_timeline` (shared with `read_playback`); `Media::seek` (remembers an admitted seek's target), `set_expanded` and the on-demand timeline re-read; `slow` (the existing slow-work log, now also around the open-edge read); `now_ticks` made `pub`. Tests. |
| `src/host.rs` | `Surface::played` (the row last drawn) and `Surface::expanded`; `paint_notch` computes the row only when the panel row is revealed; `Desktop::refresh_progress` (repaint on change, next deadline, `progress_due`/`progress_tick` floor anchor); `update` reports open state to media and joins the progress wait to the clock wait; `click_control` seeks on a bar click after the controls miss. |
| `src/layout.rs` | `media_progress` (bar target), `media_seek_at`, and one shared `panel_offset` hit-test used by both the controls and the bar. Test. |
| `src/graphics.rs` | `NotchScene::played`; `progress_row` adds the two times to the panel's lines and plans the bar's track and fill; `Line::text` becomes `Cow` so formatted times join the existing text path. Test. |
| `src/notch_uia.rs` | `State::position`; the play/pause Button reports it as `ItemStatus`. Test. |
| `docs/design/roqa-visual-spec.md` | PROVISIONAL progress-row geometry, seek, repaint and timeline-read rules, and the ItemStatus note. |

Diff against `450d46c` (round 3): 6 source/design files, **779 insertions, 58 deletions** (`artifacts/m4-progress-1/round-3/candidate.diff`). Round 1 was 590/54, round 2 746/58.

## Decisions

1. **On-demand timeline reads, no subscription.** Council fix 1 removed `TimelinePropertiesChanged` to cut playback wakes; this keeps it removed. The timeline is read:
   - with every playback-info read (existing `read_playback`, which fires on play, pause, rate and status changes);
   - once when any visible notch starts opening (`Media::set_expanded`, only for a shown track);
   - while a notch is open, after a media-properties change (track change) or after any command completes (a seek, skip or play/pause), unless the same wake already read playback info.

   Between reads the position is extrapolated per rule 4. This is correct whenever the player's last timeline report was accurate at its `LastUpdatedTime`, which is what the platform contract states ("current as of LastUpdatedTime"). It can be wrong when a player moves the position without raising playback-info or properties events and without a ROQA command (for example a seek made inside the player while the panel is open). Then the row stays extrapolated from the older report until the next read: the next open, track change, command or playback change. Recorded as a limit and in the native plan, not fixed by adding a subscription.
2. **Gate.** A notch is "expanded" when it is visible and its motion target is open (`motion.target() >= 1`), as UIA's `open` already uses. Hidden and fullscreen-suppressed notches settle at rest in `set_visible`, so they are never expanded. The progress deadline exists only for an expanded notch showing a track that is `Playing`, with `rate > 0`, a `LastUpdatedTime` and a length (`EndTime > StartTime`). Every other state yields no deadline (`INFINITE`), recomputed every pass, so nothing stale remains.
3. **Zero collapsed work.** `refresh_progress` iterates only expanded notches; with none, it reads no clock and calls nothing. `paint_notch` computes the row only when `motion::reveal(progress) > 0`, the same point at which the controls appear. `set_expanded` is one boolean check per pass and reads GSMTC only on the closed-to-open edge.
4. **Deadline (revised in rounds 2 and 3, R1-IMP-1 and R2-IMP-1).** `Track::next_tick(now, width, floor)` takes the time to the next whole second of the elapsed text and to the next physical pixel of the bar's floored fill (each capped at the end of the track), divided by the rate. At rate 1 or below it returns the second, unless the pixel deferred to the floor (`max(pixel, floor)`) still leaves a full second before the second, in which case it returns that pixel; above rate 1, `max(min(second, pixel), floor)`. The host passes `floor` = one second after `Desktop::progress_tick`, the **scheduled** time of the last tick that came due (`progress_due`), adds the wait to `now`, takes the minimum over open notches and rounds up with `milliseconds_until`. It joins the `event_loop` minimum through `update`'s returned wait.
   - Round 1 anchored the floor to the late paint instant, so wake latency accumulated and the shown second slipped and periodically skipped (reviewer's static derivation). Anchoring to the scheduled time, and never deferring a whole second at rate ≤ 1, removes that. Round 2 still allowed a pixel tick just before a second at rates between 0 and 1 (about 1.8 ticks a second at 0.9×, R2-IMP-1); round 3 takes a pixel-only tick only when the next second is a full second after it. `late_wakes_never_skip_or_delay_a_shown_second_and_ticks_stay_a_second_apart` simulates 15 ms-late wakes at 1×, 0.9×, 0.75× and 0.5× over long and short tracks and asserts no skipped second, each second drawn within the wake latency of its boundary, consecutive scheduled ticks at least 1 s apart after the first, and at most (length ÷ rate) + 2 ticks. Temporarily restoring the round-2 rule makes this test fail (checked, then reverted).
   - Reading of "at least 1 s": in steady playback, every tick after the first is at least 1 s after the previous scheduled tick, at any rate. The first tick after opening (or after a pause) is not floored, because no earlier tick exists. After a seek, resume, rate change or track change while open, one tick can come less than 1 s after the previous one, once per such event, never repeatedly (R3-NIT-2). Pixel-only changes faster than that are batched into a later tick, so the bar can trail its exact pixel by one pixel for up to about 2 s between 0.5× and 1× (about 1.6 s at 0.6×) and under a second at 1× (R3-NIT-1).
   - Reported lengths over `MAX_LENGTH` (1000 h) count as none (R1-IMP-3), so ticks × pixels stays far inside i64 and plain i64 math is used. `i128` cost 3 KB of binary (measured).
5. **Repaint only on change.** Each pass, an expanded, non-animating notch is repainted only if its computed `Progress` (shown seconds, total seconds, filled pixels) differs from the one it last drew. This covers ticks and also event-driven changes such as a completed seek, which repaint at once. Running animations paint every frame anyway and are skipped.
6. **Seek.** A click or tap that misses the three controls but lands in the bar's target maps its x offset to a fraction (left edge 0, `(width − 1)/width` at the last pixel) and calls `TryChangePlaybackPositionAsync(start + fraction × length)`, clamped to `[StartTime, EndTime]`. It uses the existing global command slot (`admit_command`, one-shot slot, wake), so it obeys the existing "one command in flight, presses ignored meanwhile" shortcut. An admitted seek remembers its target and request time (`Media::seek`). On `Ok(true)` for the current generation, after the timeline re-read, `Track::sought` shows the target from now if the read still predates the request (R1-IMP-2): a player may report its new timeline only after the completion, and no later read would come. It is gated on `shown()`, a known length and `IsPlaybackPositionEnabled` (the pinned binding's name, verified in `windows-0.62.2/src/Windows/Media/Control/mod.rs:473`; `TryChangePlaybackPositionAsync(i64)` at `:149`). No drag, hover or keyboard seek (decision 6).
7. **Hit-testing.** Controls and bar share `layout::panel_offset`: hidden until `reveal > 0`, and clipped to the part the body already shows while opening. The bar's target (y 50–64) ends above the controls row (y 66), and a test scans the whole panel at 100, 150 and 200 % for any point that hits both.
8. **Accessibility.** The play/pause Button's `ItemStatus` is the drawn position, such as "1:23 of 4:56", and empty while no row is drawn. Read-only: no RangeValue, no arrow keys. No property-changed event is raised as it advances (no extra UIA traffic per second). Whether Narrator reads ItemStatus on focus is NOT RUN.
9. **Geometry.** The addendum reserved rows 50–64 without pinning them. Chosen minimal placement, marked PROVISIONAL in `roqa-visual-spec.md`: elapsed 11 DIP Regular white 60 % from x 100 to 136 (leading), bar target 164 × 14 at (140, 50) drawn 4 DIP tall centred (track white 20 %, fill white, radius 2), total from 308 to 344 trailing. It fades in with the controls and keeps its panel place while the body morphs.
10. **Slow-read log.** The open-edge `GetTimelineProperties` read is timed with the same 8 ms threshold as the existing refresh log, through one shared `slow` helper (R1-MIN-2).
11. **Out-of-line refresh.** `refresh_progress` is `#[inline(never)]`: inlined into `update` it cost about 12 KB more release binary (measured 765,952 → 754,176 B with no other change).

## Footprint

| Build | Bytes | SHA-256 |
|---|---|---|
| Recorded `450d46c` (council fix worktree) | 740,352 | `CB88D313…FE8438` |
| `450d46c` rebuilt in this worktree path | 740,864 | `681243CE…F6324C9` |
| Round 1 | 754,176 | `E10F264F6C8637D742C6206C2113DEA6F532A6BBA645F9C6070B86566EEC5AAC` |
| Round 2 | 755,712 | `9493AF71CA0B4CA121E8CD16DB332D573F8DF04EB9E7EB410A55705C51D3FA32` |
| **This candidate, round 3** | **755,712** | **`0EFFB9E65E13EEAAD90BBA737CBFF761E34EA880347CB7BC6A5E5546DE0D6C2E`** |

Delta (round 3, same size as round 2): **+15,360 B (+2.07 %)** against the recorded 740,352 B; **+14,848 B (+2.00 %)** against the same-path rebuild; +1,536 B over round 1 for the review fixes. The 512 B base difference comes from building in a different worktree path (not investigated further). Round-1 bisection by temporary stubs (reverted): row drawing about 1.5 KB, UIA position text about 2 KB, seek about 2 KB, the rest is the refresh/deadline path. No runtime CPU, wake, memory or paint measurement is implied.

Runtime cost by design (unmeasured): collapsed, hidden or fullscreen-suppressed, zero progress reads, deadlines and paints. Open and playing, at most one deadline-driven repaint per second per open notch in steady playback (after the first, and except once after a seek, resume, rate or track change), plus one wall-clock read and integer math per open notch per loop pass. One extra GSMTC getter (`GetTimelineProperties`) per open, and per track change or completed command while open.

## Verification

Pinned project-local toolchain through an ignored `.tools` junction to the root checkout's `.tools`; global PATH unchanged. No roqa.exe was running before the runs.

- `scripts/check.ps1`: PASS (fmt, clippy `--locked --all-targets -D warnings`, tests).
- Round 1: `scripts/verify.ps1 -EvidenceDirectory artifacts/m4-progress-1/round-1/verify`: PASS, 78 tests passed, 0 failed (71 on base, +7).
- Round 2: `scripts/verify.ps1 -EvidenceDirectory artifacts/m4-progress-1/round-2/verify`: PASS, 82 tests passed, 0 failed (+11 over base).
- **Round 3: `scripts/verify.ps1 -EvidenceDirectory artifacts/m4-progress-1/round-3/verify`: PASS**, checks and locked release build exit 0, **82 tests passed, 0 failed** (+11 over base; round 3 changed tests, not their count).

New tests:

- `media::progress_shows_whole_seconds_and_floored_pixels_and_needs_a_duration`: text and pixel flooring, clamps at both ends, a late `StartTime`, `m:ss` past an hour, and no row, deadline or seek without a usable `EndTime`.
- `media::without_a_floor_ticks_land_on_the_next_second_or_a_pixel_well_before_it`: second first, fast pixels waiting for the second, a pixel well before the second at 0.5×, both together, exactly on a boundary, rates 2 and 0.5, the end as the last change, paused, rate 0, negative rate, no `LastUpdatedTime`, zero-width bar.
- `media::the_floor_defers_pixels_and_fast_rates_but_never_a_second` (rounds 2 and 3).
- `media::late_wakes_never_skip_or_delay_a_shown_second_and_ticks_stay_a_second_apart` (rounds 2 and 3): the floor-anchor protocol the host follows, simulated with late wakes at four rates. It reimplements the host's two-line anchor arithmetic rather than calling it (R2-NIT-2, deferred).
- `media::absurd_lengths_count_as_none` (rounds 2 and 3): `EndTime` = `i64::MAX` with several starts, and the 1000 h cap a second before its end on a 4096 px bar (wider than any real one; the i64 margin holds to about 256,000 px).
- `media::an_accepted_seek_shows_its_target_until_the_player_reports_a_newer_timeline` (round 2).
- `media::seek_targets_clamp_to_the_timeline`: edges and out-of-range fractions.
- `media::seeks_need_a_session_a_shown_track_and_a_player_that_accepts_them`: no session means no command and no read.
- `layout::the_progress_bar_maps_clicks_to_its_fraction_beside_the_controls`: x-to-fraction mapping at the edges, misses, no overlap with the controls at three scales, hidden while collapsed, clipped while opening.
- `graphics::the_panel_draws_elapsed_bar_and_total_between_the_text_and_the_controls`: exact rectangles at 100 %, inside the body at 150 and 200 %, fade-in with the controls, nothing collapsed or without a row.
- `notch_uia::play_pause_reports_the_drawn_position_as_its_status`.

The host-side gate (`Surface::expanded`, `refresh_progress`) needs live surfaces and is covered by the pure pieces above, not by a host test (R1-MIN-1, which the round-1 reviewer judged acceptable as statically reviewed). The anchor bookkeeping (`progress_due` to `progress_tick`) and the per-notch loop are reviewed statically; the floor policy itself is tested through `next_tick` and the late-wake simulation.

## Limits and deferred

- **Hung player.** `GetTimelineProperties` is synchronous on the host thread, like the existing `GetPlaybackInfo`/`GetTimelineProperties` in `read_playback` (UNVERIFIED item 3 of the preparation). This adds call sites (on open; on a track change or completed command while open) but no new class of risk. Both the refresh path and the open-edge read log `Media: slow …` over 8 ms.
- **Seek freshness heuristic (R2-MIN-1).** `Track::sought` trusts a completion read whose `LastUpdatedTime` is at or after the request. A player that pushes its timeline periodically could push once after the request but before applying the seek, then not again before the read; the row would then keep the pre-seek position until the next read. The window is short; native evidence item 3 must include such a player.
- **Track change while open (R1-IMP-2, remaining part).** The re-read on `MediaPropertiesChanged` can still see the old track's timeline if the player updates it later and raises no playback-info change; the row then keeps the old timeline until the next read. There is no local fix without a subscription; native evidence item 4 must check it with real players.
- **External seeks while open** are not seen until the next read (decision 1).
- **Seek while another command is in flight** is ignored (existing shortcut).
- **ItemStatus** has no change event; Narrator behaviour NOT RUN.
- **Times wider than their 36 DIP columns** (about 100 minutes and more at 11 DIP) end in an ellipsis.
- **`PlaybackRate` unset** is still taken as 1× (existing shortcut in `read_playback`); whether real players populate `PlaybackRate` and `LastUpdatedTime` remains UNVERIFIED item 6.
- Deferred findings from council fix 1 (R4-MIN-1 Changing-status flicker, desktop-foreground R3-MIN-1, nits) are untouched.

## Native evidence plan (all NOT RUN)

**Every native or GUI run needs fresh, explicit, desktop-free confirmation from the user through the Chief, and a check of the foreground window first, because scripts inject input and move the cursor. Prior permission never carries over.** Freeze and verify the exact binary hash first; only the owned test process may be stopped, preferring `--quit`. No settings, display, Explorer, TDR or remote-session changes without separate authorization.

1. **Wake and paint counts.** With one player playing a long track: 2 minutes collapsed, then 2 minutes with the panel held open, then 2 minutes collapsed again. Repeat the open phase at 0.75× or 0.9× where the player allows it (R2-IMP-1). Record media `wakes=`, loop wakes and notch paint counts per phase. Expected: collapsed adds no progress wakes or paints (only the clock's per-minute paint); open adds at most one progress repaint per second; collapse ends them at once.
2. **Gate cases.** While open: pause (ticks stop, row holds still), resume, set rate 2× where the player allows it, and play a stream without a length (no row, no seek, no deadline). Fullscreen a video on the notch's monitor while open (notch hidden, no ticks), then restore.
3. **Seek.** Click near the start, middle and end of the bar in two players (for example a browser and a native player), including one that updates its timeline periodically (R2-MIN-1). Record that the position moves, the row repaints once on completion, and that a player with `IsPlaybackPositionEnabled` false ignores clicks. Also click during the opening animation and on the controls' edges.
4. **Timeline freshness.** Open the panel mid-track (row matches the player), change track while open, seek inside the player while open (known limit: the row stays stale until the next read), and leave the panel open across a track end.
5. **Accessibility.** With Narrator, focus play/pause while open and record whether "1:23 of 4:56" is read; confirm no arrow-key seeking and no announcement storm.
6. **Hung player.** Reuse the existing hung-provider harness to observe whether opening the panel blocks on `GetTimelineProperties`.
7. **Regression.** Rerun the five suites (`smoke`, `topology-check`, `picker-smoke`, `catalog-smoke`, `packaged-hung-check`) and a 30 s idle sample against this hash, each with its own confirmation.

## Review rounds

Recorded below as they complete. The manifest at `artifacts/m4-progress-1/round-N/inputs.sha256` freezes the inputs of each round; this packet and the review reports are mutable records outside it.

### Round 1

Manifest `artifacts/m4-progress-1/round-1/inputs.sha256`: 133 raw-byte inputs, SHA-256 `4C6C19DBFF948F9C5D1A282F4EFB1A1038E75DE7C3B51F969ED51C1BA3BC2EE2`. Fresh independent Claude Opus 5.5 High clean-code-guard review (read-only, verified 133/133 at start and end): **CHANGES REQUIRED**, 0 critical, 3 important, 2 minor, 2 nits. Report `artifacts/m4-progress-1/round-1/review-opus.md`, SHA-256 `2ADEED04AAD0D327EAACEBAB0A926E978FD9E6FD2E8BEB2E5FB7F7729479FB6CF`.

| Finding | Disposition (author fix, round 2) |
|---|---|
| R1-IMP-1: 1 s floor anchored to the late paint drifts and skips seconds; untested | FIXED: `next_tick` never defers a whole second at rate ≤ 1; the floor counts from the scheduled tick (`progress_due` → `progress_tick`); late-wake simulation test. |
| R1-IMP-2: seek-completion read may predate the seek | FIXED for seeks: `Track::sought` shows the target until a newer timeline arrives; tested. The track-change variant is a recorded limit and native gate (item 4). |
| R1-IMP-3: overflow on extreme `EndTime` | FIXED: lengths over 1000 h count as none; tested with `i64::MAX`. |
| R1-MIN-1: no host gate test | Accepted by the reviewer as statically reviewed; unchanged. |
| R1-MIN-2: open-edge read not timed | FIXED: shared `slow` helper logs both. |
| R1-NIT-1: `progress_row` had five parameters | FIXED: it takes the scene. |
| R1-NIT-2: position cloned on every property query | FIXED: cloned only for `ItemStatus`. |

### Round 2

Manifest `artifacts/m4-progress-1/round-2/inputs.sha256`: 133 raw-byte inputs, SHA-256 `F4CD9127DD7D618E3945339CBB182136D9D0B8725DCE579901873185E5CFBC6F`. Fresh independent Claude Opus 5.5 High clean-code-guard review (read-only, 133/133 at start and end): **CHANGES REQUIRED**, 0 critical, 1 important, 1 minor, 2 nits; all round-1 findings resolved (IMP-1 at rate 1, IMP-2 as specified). Report `artifacts/m4-progress-1/round-2/review-opus.md`, SHA-256 `AACF8CDAD6DDF9EE294EA30106809F4B320456DB3FFE5FDB98EFF1D6435C03B9`.

| Finding | Disposition (author fix, round 3) |
|---|---|
| R2-IMP-1: below rate 1, two ticks per second period, as little as 0.11 s apart | FIXED with the reviewer's rule; simulation at 0.9×, 0.75× and 0.5× asserts ticks at least 1 s apart; packet and addendum claims corrected. |
| R2-MIN-1: periodic timeline push can fool `sought` | Recorded as a limit; native item 3 covers such a player. No code change. |
| R2-NIT-1: 2^20-pixel test claim overstated | FIXED: realistic-but-wide 4096 px bar, tested a second before the cap's end. |
| R2-NIT-2: simulation duplicates the host anchor arithmetic | DEFERRED: two lines; a shared helper adds code for little gain (the reviewer judged it acceptable). |

### Round 3 (final allowed round)

Manifest `artifacts/m4-progress-1/round-3/inputs.sha256`: 133 raw-byte inputs, SHA-256 `E96A4B32FDAB6A069FC6062B1683AA0C172C0137C460E65B33614B047A74DD36`. Fresh independent Claude Opus 5.5 High clean-code-guard review (read-only, 133/133 at start and end, uncommitted diff byte-identical to `candidate.diff`): **ACCEPT**, 0 critical, 0 important, 0 minor, 3 nits. R2-IMP-1 resolved (hand-traced at 0.5×, 0.75× and 0.9×: no skipped or delayed second, no spin, steady ticks at least 1 s apart, every change drawn); all round-1 fixes still hold; R2-MIN-1, R2-NIT-1 and R2-NIT-2 dispositions accepted. Report `artifacts/m4-progress-1/round-3/review-opus.md`, SHA-256 `BE4377F7B4C79DAD30C28499EE2ECD1F1F9EF26E15ABF60C87A93577D36A667A`. The author revalidated all 133 inputs after the review; no source, test or addendum edit after the freeze.

| Finding | Disposition |
|---|---|
| R3-NIT-1: the bar can trail its exact pixel by up to about 2 s between 0.5× and 1×, not "under a second" | Packet wording corrected above (packet is outside the manifest). No code change. |
| R3-NIT-2: "at least a second apart after the first" fails once after a seek, resume, rate or track change while open | Packet wording corrected above. The same sentence in the frozen addendum (`roqa-visual-spec.md`, Repaints, "Open panel") is DEFERRED to avoid changing accepted inputs; it should read "in steady playback". |
| R3-NIT-3: no late-wake simulation case above 1× | DEFERRED (optional); the rate > 1 path is covered by a single-call test. |

This static ACCEPT is not native, milestone or council acceptance. The author approves nothing; all native evidence below remains NOT RUN.
