# M4 slice 5 candidate 1: album art

Author: Claude Opus 5.5 worker thread launched by the ROQA Chief Thread, 2026-11-11. Branch `feat/m4-album-art`, base **450d46c** (equal to origin/main). There is no commit and no push; the Chief lands it. The author runs checks but approves nothing. Slice 4 (progress and seek) is being built in parallel on another branch; this candidate touches no progress or seek code.

Scope follows `docs/development/m4-media-preparation.md` rule 5 and slice 5, and the PROVISIONAL "Cover accent" addendum in `docs/design/roqa-visual-spec.md`. All native evidence is **NOT RUN**.

This packet describes **round 3**, after the round-1 and round-2 fixes. It is a mutable reporting artifact and sits outside its own manifest.

## What changed

| File | Change |
|---|---|
| `src/media.rs` | One `Art` entry (key, thumbnail read, pixel slot, seen flag). `read_art` runs when a fetch for the current generation lands with a new (generation, title, artist) and a non-empty title or artist. `hand_off` is the completion side. `art()` exposes the art; `refresh(cover)` reports its arrival once and releases a finished read; `end_session` drops the entry; `start(jobs)` takes the worker queue. |
| `src/icons.rs` | The existing `roqa-icons` STA worker now takes `Job::Icon` or `Job::Art`. Art: resolve the `AgileReference`, reject `Size()` > 4 MiB, WIC decode, frame side ≤ 3072, centred square crop, PBGRA convert, Fant scale to the drawn side (≤ 256), then call `done` only on success. `IconService::jobs()` hands out the queue. |
| `src/graphics.rs` | `NotchScene::art` and `COVER_DIPS` (72, shared with the host). With art, a bitmap brush fills the cover's rounded rect (no layer); without it, the unchanged slot and glyph. A pill-only corner badge. The dock's bitmap upload moved into a shared `upload` helper, which the art uses too. |
| `src/host.rs` | `media` is declared before `icons`, so media's queue handle drops first and the worker can stop. `cover_pixels()` gives the panel cover on the densest notch. The scene passes `media.art()`. |
| `Cargo.toml` | Features `Storage_Streams` and `Win32_Graphics_Imaging` (below). |
| `docs/design/roqa-visual-spec.md` | The cover-slot and repaint lines now describe art, the badge, and the one arrival paint. |

The source diff is 675 insertions and 75 deletions across 6 tracked files, plus this packet. The exact diff is `artifacts/m4-art-1/round-3/candidate.diff`.

## Decisions

1. **Flow.**
   - A properties fetch lands for the current generation with a new key (generation, title, artist).
   - The host calls `Thumbnail().OpenReadAsync()` and attaches a `when` completion.
   - The completion (any thread) casts the stream to `IRandomAccessStream`, wraps it in `AgileReference::new`, `try_send`s `Job::Art` to the icon worker, and raises the media wake.
   - The worker decodes. On success, the `done` built in `read_art` fills the entry's `OnceLock` slot and raises the existing media wake (`READY`).
   - On its next wake, the host's `refresh` drops the finished read. When the slot has filled and the track is shown, it repaints once.

   No new thread, timer, deadline or poll. Callbacks never block, log or touch host state.
2. **One entry.** `Art` lives in `Media`. A new key replaces the whole entry: dropping the old one cancels its read (a request only), and its slot is orphaned, so a late decode for an older key lands nowhere. `end_session` drops the entry. That is the stale-result discard; no key comparison is needed at paint time.
3. **Finished reads are released (round-1 IMP-1, round-2 IMP-1).** A completed `IAsyncOperation` keeps its result stream while referenced, so the completion wakes the host. On that wake, `refresh` drops its reference once `Status()` is no longer `Started`. It never calls `Close()`: WinRT reports `Completed` before invoking the handler, and windows-future's `when` calls `GetResults()` inside that handler, so closing could fail a read that just succeeded (round 2). Dropping a reference is always safe. Only the worker's own reference keeps the stream alive while it decodes, and it releases that before replying. This costs one extra wake per track and no paint.
4. **Never retry.** Recording the key comes before any step that can fail. Each of these leaves the slot empty and the no-art layout drawn:
   - no thumbnail (`Thumbnail()` errors or is null);
   - `OpenReadAsync` or `when` fails, or the read completes with an error;
   - the `AgileReference` cannot be created, or the queue is full;
   - resolving fails on the worker;
   - the stream is over 4 MiB, or the frame is 0 or larger than 3072 a side;
   - WIC fails.

   The same key never reads again. A fetch with an empty title and artist is never shown, so it reads no art (round-1 NIT-4).
5. **Arrival repaint.** The worker fills the slot before it wakes the host, so a before/after comparison within one `refresh` would miss the arrival. A `seen` flag reports it exactly once. It repaints only when the track is shown (round-2 NIT-1). Collapsed static playback adds exactly this one media paint per track; failures and the read-release wake add none.
6. **Caps.**
   - `Size()` above 4 MiB is rejected before any WIC object is created.
   - The decoded side is `min(drawn side, crop side, 256)`. The drawn side is `COVER_DIPS` at the densest notch DPI, which already includes text scale. While the notch is hidden there is no notch surface, so it falls back to `COVER_DIPS` at 96 DPI; before round 2's MIN-1 fix it was 0, which lost that track's art for good. At most 256 × 256 × 4 = 256 KiB retained.
   - **Added beyond the brief:** a frame-side ceiling, checked from the header before any pixels are pulled, because a sub-4 MiB stream can declare a huge frame. Round 1 lowered it from 4096 to **3072**, which still fits 3000 × 3000 store artwork (round-1 MIN-2). It bounds a whole-frame WIC buffer to about 36 MiB of BGRA, transient. The real transient peak is unmeasured.
7. **Crop.** A centred square crop (`IWICBitmapClipper`), so 16:9 video thumbnails fill the square slot without distortion. The conversion to PBGRA happens before Fant scaling, so transparent edges do not get dark fringes.
8. **Paint.** An `ID2D1BitmapBrush1` with a scale and translate transform fills the cover's `D2D1_ROUNDED_RECT`, which clips it exactly with antialiased corners and no layer.
   - Interpolation is created as LINEAR, then `SetInterpolationMode1(HIGH_QUALITY_CUBIC)` is called. Microsoft Learn: an invalid mode is "silently ignore[d]", so a brush that does not support cubic cannot fail the paint.
   - The art is drawn at the paused 60 % `dim`, like the glyph. The mockup uses 55 %; the existing token is kept.
   - Geometry is unchanged: 20 at x 10 with radius 5 in the pill, 72 at (16, 14) with radius 10 in the panel. A test asserts the art cover equals the no-art cover.
9. **Badge (drawn: cheap).** One rounded fill plus the existing `fill_glyph`. It appears only with art: a black 11 DIP circle whose right and bottom edges are 4 DIP past the cover's, carrying a 6 DIP white play-state glyph. The values come from the mockup's CSS (`artifacts/m4-mockups/notch-variants.html` in the root checkout, outside version control). It fades by `1 − smooth(p)`, because the mockup draws it only on the 20 px cover and the panel's play/pause control carries the state there.
10. **Teardown.** `Media` holds a clone of the queue sender, so `Desktop` now declares `media` before `icons`. Otherwise `IconService::drop` would wait out its 1 s bound and detach the worker on every quit.

## Footprint

- Release `target/release/roqa.exe`: **758,272 bytes**, SHA-256 **346492C0BA5EB9F1665BCF2AA3E702F8BDBFB5F2E0DAB59DB636FA306D675DB6**. That is **+17,920 B (+2.42 %)** over `450d46c`'s 740,352 B (base size as supplied by the Chief; the base was not rebuilt here). Round 1 was 757,760 B; round 2 was 758,272 B with SHA-256 `D37FCADE…2586B3`.
- **New `windows` features:**
  - `Storage_Streams` gates `GlobalSystemMediaTransportControlsSessionMediaProperties::Thumbnail`, `IRandomAccessStreamReference::OpenReadAsync` and `IRandomAccessStream::Size`. It was planned for slice 1 but first needed here. The prep doc says it implies `Storage`.
  - `Win32_Graphics_Imaging` gates WIC: `IWICImagingFactory`, the decoder, clipper, format converter and scaler, and `GUID_WICPixelFormat32bppPBGRA`. Slice 5 is the slice the prep doc assigns it to.

  No new crate. `Cargo.lock` is unchanged.
- **Runtime.** No new thread (the existing `roqa-icons` worker decodes), no timer, deadline or poll.
  - Retained memory is at most one decoded entry of ≤256 KiB, plus the key strings (≤2 × 512 UTF-16 units).
  - The opened stream is no longer held after its read finishes.
  - WIC load and per-decode transient memory are **unmeasured**.

## Checks

`scripts/verify.ps1 -EvidenceDirectory artifacts/m4-art-1/round-3/verify`: **PASS**. Checks exit 0 (fmt, clippy `--locked --all-targets -D warnings`, tests) and the release build exit 0. **78 tests passed**, 0 failed; the base had 71. The pinned `.tools` was used through a worktree junction, and global PATH was unchanged. No native or GUI run. Before verify, no `roqa.exe` was running.

New tests:

| Test | Covers |
|---|---|
| `icons::art_crops_the_centred_square_and_caps_the_decoded_side` | Crop, no upscaling, the 256 ceiling, and rejection of empty or oversized frames and a zero side |
| `icons::art_over_the_stream_cap_or_unresolved_is_skipped_before_decoding` | An in-memory stream at 4 MiB + 1 is rejected with the size error; at the cap it reaches the decoder. An unresolvable reference gives `None`. |
| `icons::art_decodes_the_centred_square_as_premultiplied_bgra` (round 2) | A hand-built 2 × 1 BMP goes through the real WIC clip, convert, scale and `CopyPixels` path to a 1 × 1 opaque PBGRA pixel |
| `icons::the_worker_replies_to_art_only_when_decoding_succeeds` (round 2) | A real `IconService` worker in its STA. An empty stream sends no reply; the BMP job after it replies with its pixels. Resolving the `AgileReference` across threads works. |
| `media::album_art_is_one_entry_per_track_and_a_failed_track_is_never_read_again` | A missing thumbnail is read once for its key. A real `RandomAccessStreamReference` goes through `OpenReadAsync`, `when` and `AgileReference` to the queue at the drawn side, and resolves. A stale decode after a key change is discarded. The finished read is released (round 2). Arrival repaints exactly once. Ending the session drops the entry. |
| `media::album_art_is_skipped_without_a_reference_or_queue_room` | A failed reference creation and a full queue both skip, with no job |
| `graphics::album_art_fills_the_cover_slot_and_only_the_pill_carries_a_badge` | Art equals the no-art cover, badge geometry at 100 %, no badge in the panel, no badge without art |

## Known limits and deferred items

- **Never retry, per the rule (round-1 MIN-1).**
  - A player may publish title and artist before the thumbnail, under the same key. At the first fetch it then reports either no thumbnail, so the track has no art for its whole length, or **the previous track's thumbnail**. In the second case the wrong cover is shown for the whole track and never corrected.
  - How real players behave is UNVERIFIED. Browser MediaSession artwork loads asynchronously, so test with Chromium or Edge.
  - If it reproduces, a possible follow-up (needing Chief approval) is to leave the key unrecorded only when `Thumbnail()` is null. Decode and stream failures would stay never-retry.
- **Shared worker (round-1 MIN-4).**
  - Art waits behind any queued dock-icon extractions (capacity 64). A full queue skips that track's art for good.
  - A hung player stream blocks the shared `roqa-icons` worker, because `Size()` and the WIC reads are synchronous. Dock icons for apps opened later then never load in that session, and quit logs the 1 s detach.
  - Both are accepted trade-offs of reusing the worker (the prep doc lists "can reading a thumbnail stream block" as UNVERIFIED).
- **Shutdown.** A thumbnail read still in flight at quit holds a sender clone inside its completion until it completes or is released. The existing 1 s worker-shutdown bound then applies.
- **DPI.** The decode size is fixed per track. A DPI or text-scale change resamples until the next track (`shortcut:` in `host.rs`).
- **Bitmap upload.** The art bitmap is uploaded on each notch paint, as the dock does with its icons. That covers arrival, track changes, clock minutes and open/close frames (`shortcut:` in `graphics.rs`).
- **Size cap.** It relies on `Size()`. A stream reporting 0 or a wrong size is bounded only by the decoder and the 3072 frame cap.
- **Cubic sampling.** Whether `ID2D1BitmapBrush1` actually honours cubic is UNVERIFIED. If it stays linear, the pill's 3.6× downscale may alias.
- **Cover thread.** Whether real GSMTC thumbnail streams are agile is UNVERIFIED.
- Not built: a multi-entry or disk cache, blur or colour extraction (non-goals), and RTL mirroring of the cover (already deferred in the spec).

## Native evidence plan (all NOT RUN)

Every run needs fresh, explicit, desktop-free confirmation from the user through the Chief, and a foreground-window check. Stop only the owned test process, preferring `--quit`. Leave the taskbar and settings unchanged. Freeze and verify the binary hash first.

1. Art from two real players, including a Chromium or Edge MediaSession tab and a desktop player:
   - art fills the pill and panel slots, nothing moves, and the corners are clipped;
   - the pill badge is right for play and pause, and the panel has no badge;
   - change tracks repeatedly and watch for missing art or the previous track's cover (MIN-1);
   - check pill sharpness (cubic versus linear).
2. Track churn: 20 or more track changes, including one with a large (> 4 MiB) thumbnail. Record private bytes and handles before and after; one entry is retained and no finished stream is held.
3. Over-cap or invalid art (round-2 NIT-2: failures on the completion side are silent by design, because callbacks never log. These are an error result from the read, a failed `AgileReference`, and a full or closed queue. So a missing log line is not a pass or a fail by itself):
   - a thumbnail over 4 MiB, a corrupt image, a non-square image, and a sub-4 MiB progressive JPEG at 3072 × 3072;
   - expect the no-art layout where appropriate and no retry. Host-side and worker-side failures log one `Media: no album art:` or `Album art skipped:` line per track;
   - record the private-bytes peak during the 3072² decode.
4. Paint counts while collapsed and static across a minute boundary: the clock's minute paint plus exactly one art-arrival paint per track.
5. Mixed DPI and text scale: decode size and sharpness at 100, 150 and 200 %.
6. Quit while a read is in flight, and with a stalled player stream if one can be produced. Check whether the icon worker logs `still extracting after 1 s`, and whether dock icons still load after a stall (MIN-4).
7. Device-loss rebuild with art shown: art reappears from the retained pixels.

## Review

Each round is a fresh Claude Opus 5.5 High clean-code-guard review delegated by the author, with at most 3 rounds and no Sol. The author approves nothing.

| Round | Inputs | Verdict | Report |
|---|---|---|---|
| 1 | `artifacts/m4-art-1/round-1/inputs.sha256`, 133 entries, SHA-256 `63053607950b3c8f98555ed1b50318e0ce6eb28e3e7863f5049586596e7ee39d` | **CHANGES REQUIRED**: 0 critical, 1 important, 4 minor, 4 nits. Inputs verified at start and end. | `artifacts/m4-art-1/round-1/review-opus.md`, SHA-256 `ef76ab43bac517e9bf04073339ab77aba14a806afec829d93cf6828050aca5af` |

Round-1 dispositions:

| Finding | Disposition |
|---|---|
| IMP-1: a finished read keeps its stream | **Fixed.** The completion always wakes, and `refresh` drops the read once it is not `Started`. A test asserts the release. In round 2 `refresh` also closed the read; round 3 removed that `Close()` (see round-2 IMP-1). |
| MIN-1: same-key late or stale thumbnail | **Deferred** per the never-retry rule. Added to known limits and native item 1, with a possible follow-up for the Chief. |
| MIN-2: large transient decode | **Fixed in part.** The frame cap is 4096 → 3072, and the transient is measured in native item 3. |
| MIN-3: no real decode or worker test | **Fixed** with the two new `icons` tests above. |
| MIN-4: a hung stream blocks the shared worker | **Deferred** as a design trade-off. Added to known limits and native item 6. |
| NIT-1: `hand_off` has 5 parameters | **Fixed.** `done` is built in `read_art`; `hand_off` takes 4. |
| NIT-2: stale cost doc | **Fixed.** The doc names the caller's surface scan. |
| NIT-3: 72 duplicated | **Fixed** with `graphics::COVER_DIPS`, used by both. |
| NIT-4: art read for an unshown track | **Fixed.** It is skipped when title and artist are both empty. |

| Round | Inputs | Verdict | Report |
|---|---|---|---|
| 2 | `artifacts/m4-art-1/round-2/inputs.sha256`, 135 entries, SHA-256 `5398def9f8af4dda66758d51dde2cd384b2dc159fcd32786950db2bc64cc5747` | **CHANGES REQUIRED**: 0 critical, 1 important, 1 minor, 3 nits. Inputs verified at start, before writing and after. All round-1 fixes and both round-1 deferrals accepted. | `artifacts/m4-art-1/round-2/review-opus.md`, SHA-256 `b884da87af9757d1a79ba81f98d00966b2fd6b5abeae41f0bfb04cabac6e9221` |

Round-2 dispositions:

| Finding | Disposition |
|---|---|
| IMP-1: `Close()` on a finished read races the completion's `GetResults()` (introduced by the round-1 fix; the round-1 reviewer withdrew its `Close()` suggestion) | **Fixed.** The read is only dropped (`take_if`), never closed; a comment records why. |
| MIN-1: with the notch hidden, the cover size is 0 and the track loses its art for good | **Fixed.** `cover_pixels()` falls back to `COVER_DIPS` at 96 DPI. |
| NIT-1: the arrival repaint fires for a track that is not shown | **Fixed.** It repaints only when `shown()`; `seen` is still set. |
| NIT-2: completion-side failures are silent, but native item 3 expected a log line for each | **Fixed in the packet.** Native item 3 now says those failures are silent by design. No counter was added. |
| NIT-3: the tests rely on the implicit MTA without saying so | **Fixed** with a comment in both WIC tests. |

| Round | Inputs | Verdict | Report |
|---|---|---|---|
| 3 (final) | `artifacts/m4-art-1/round-3/inputs.sha256`, 136 entries, SHA-256 `44212414c1dad000a17a91691d311219e9ff1c75553032c3ddf3612c4f8952aa` | **ACCEPT WITH NITS** (static code only): 0 critical, 0 important, 1 minor, 1 nit. Inputs verified at start, before writing and after. Every round-1 and round-2 finding confirmed fixed, or its deferral accepted. | `artifacts/m4-art-1/round-3/review-opus.md`, SHA-256 `ba216a42aad2406c5b3ea8f5b2d8da4dd74e65753e52031f3152c6c4195f4158` |

That round-3 task first stopped on an API rate limit. The author confirmed the inputs were unchanged, with no partial report, and started a fresh retry. The original task then resumed and completed, so the author cancelled the retry before it wrote anything; the report hash above was taken before and after the cancel. The retry approves nothing.

Round-3 dispositions. This was the last allowed round, so the inputs stay frozen and the source is unchanged after it.

| Finding | Disposition |
|---|---|
| MIN-1 (`src/icons.rs:135-143`): if the worker exits its loop early because a dock-icon result can't be delivered at quit, a queued art job's `AgileReference` is released after `CoUninitialize` | **Deferred to a reviewed follow-up.** It is quit-only and probably benign. The suggested fix is one line: `drop(request_rx);` after the loop, before `CoUninitialize`. Landing it in this candidate would mean re-freezing the inputs, which the Chief or user must authorise. |
| NIT-1: the packet's round-1 IMP-1 row still said "closes" | **Fixed in this packet** (the packet is outside the manifest). |

## Integration 1: onto slice 4 (`624e14f`)

Slice 4 (progress and seek) landed on main as `624e14f`, and the Chief asked for slice 5 to be integrated onto it as a new candidate. The branch `feat/m4-album-art` now points at **624e14f**, and the candidate is uncommitted working-tree changes, as before.

- **Accepted state preserved.** `artifacts/m4-art-1/pre-rebase.diff` is byte-identical to `round-3/candidate.diff`. `artifacts/m4-art-1/pre-rebase-src/` holds copies of the six changed files and the packet.
- **Method.**
  1. A local-only WIP commit of the accepted state, `git rebase 624e14f`, and conflict resolution.
  2. `git rebase --continue` refused, so the author ended the rebase with `git rebase --quit`, which leaves the working tree untouched. The branch was pointed at `624e14f`, and HEAD was reattached by `symbolic-ref`.
  3. The working diff was compared byte for byte before and after.

  Nothing was committed to main or pushed.
- **Conflicts and their resolution** (both slices' behaviour kept; slice 4 semantics unchanged):
  - `src/media.rs`:
    - The fields and the constructor have both slice 4's `expanded` and `seek` and slice 5's `art` and `jobs`.
    - Slice 4's `seek`, `set_expanded` and `reread_timeline` are kept as they are, before `refresh(cover)`.
    - The refresh tail uses slice 4's `slow("host refresh", started)` in place of slice 5's inline 8 ms log, then slice 5's arrival logic, then `read_art`.
  - `src/graphics.rs` `media_text`, in paint order:
    1. the cover, as art or as the neutral slot and glyph;
    2. the pill badge;
    3. slice 4's progress bar;
    4. the text lines and the controls.

    The bar's "as above" SAFETY comment now refers to the cover block.
  - `docs/design/roqa-visual-spec.md`: slice 4's repaint paragraph and its open-panel and timeline bullets are kept. Slice 5's sentence "Album art adds one paint when it arrives." comes before "Collapsed static playback otherwise keeps only the clock's per-minute paint."
  - Merged without conflicts: `host.rs` (slice 4's `set_expanded`/`refresh_progress` sit beside `refresh(self.cover_pixels())`; the field order is kept), `icons.rs` and `Cargo.toml`.
  - The new graphics test's `NotchScene` literal gains slice 4's field `played: None`. That is the only test edit.
- **Layout.** In the 360 × 120 panel the art cover is (16, 14)–(88, 86) and slice 4's bar is x 140–304, y 50–64 (`layout::MEDIA_PROGRESS`), so they don't overlap. The badge exists only while collapsed and fades out as the panel opens. Art arrival does not touch slice 4's progress deadline: arrival is one `refresh` repaint, and the open panel's tick is `refresh_progress`.
- **Round-3 MIN-1 folded in.** In the icon worker, `drop(request_rx);` now comes after the loop and before `CoUninitialize`. Queued art jobs, which hold `AgileReference`s after an early exit, are therefore released while COM is still initialized. Nothing else is new.
- **Verify.** `scripts/verify.ps1 -EvidenceDirectory artifacts/m4-art-1/integration-1/verify` passed: fmt, clippy, **89 tests** (82 from slice 4 plus 7 from slice 5), and the release build.
  - Binary: **774,144 B**, SHA-256 **1F80554FCC79A5F50153076C1F813563F90537C0092A07CE228134F01812A84D**.
  - That is **+18,432 B (+2.44 %)** over `624e14f`'s 755,712 B, a size the Chief supplied; the base was not rebuilt here. On `450d46c` the same slice added +17,920 B.
- **Review aids** in `artifacts/m4-art-1/integration-1/`:
  - `candidate.diff`: `git diff` against `624e14f`;
  - `slice4-base.diff`: `450d46c..624e14f`;
  - `diff-of-diffs.txt`: `diff -u pre-rebase.diff candidate.diff`.

### Integration 1 review

A fresh, independent Claude Opus 5.5 High clean-code-guard review, scoped to the conflict resolution and the slice-4 interaction, the one-line COM fix, and equality with the accepted round 3.

- **Verdict: ACCEPT WITH NITS** (static code only): 0 critical, 0 important, 0 minor, 1 nit.
- **Report:** `artifacts/m4-art-1/integration-1/review-opus.md`, SHA-256 `fa7b498b7ecc0d902bc78002dd352341e88b85afb9b5030a7f267e3651260e82`.
- **Inputs** were verified at the start and again before writing: manifest SHA-256 `c0880dc49b6464a38a06a1de8ddb56de7fae32f174b3ef859b5c9f97149e5413`, HEAD `624e14f`, and `git diff` byte-equal to `candidate.diff`.
- **Slice 4 semantics are unchanged:** seek, `set_expanded`, the timeline reread, `refresh_progress`, the bar hit-test and the deadlines.
  - Art arrival only raises `READY`; it starts no timeline reread or properties fetch.
  - A seek never starts an art read, because the key does not change.
  - Arrival is one repaint with no deadline.
  - The paint order is correct, and the cover and badge never reach the bar.
- **COM fix:** `drop(request_rx)` is correct and sufficient for round-3 MIN-1. A sender still alive afterwards fails `try_send` with "disconnected", and the job is dropped safely on the completion thread. That std-channel behaviour came from the reviewer's knowledge: the pinned toolchain has no `rust-src`.
- **The rest equals round 3.** Every added and removed line was compared. The only differences are the merged spec paragraph, `played: None` in the test, and the two-line COM fix.
- **ART-I1-NIT-1:** the merged `Media::refresh` is about 116 lines. It is **deferred** as an optional follow-up (move the properties-outcome handling into its own method), with no change in this candidate.
- No second integration round was needed. The author approves nothing. All native evidence remains **NOT RUN**.
