# M4 media paint candidate 1: static media notch, direction B "Cover accent"

- **Author.** Claude Opus 5.5 High worker, launched by the ROQA Chief Thread. It continued and reworked an unfinished work-in-progress by an earlier worker (thread `mcp:launch:0b29a3e5cfe01293d8343efa03297cda2bbd62d5a816970b23fc29ee1d0cdfd6`, which ran out of context). Neither author approves anything.
- **Base.** `478d9e0` (origin/main, M4 slice 1). Uncommitted on `feat/m4-media-paint`.
- **Changed files.** `src/graphics.rs`, `src/host.rs`, `src/layout.rs`, `src/media.rs`, `scripts/smoke.ps1`, `scripts/topology-check.ps1`, `docs/design/roqa-visual-spec.md`, `docs/development/m4-media-preparation.md`, and this packet.
- **Final manifest.** `artifacts/m4-media-paint-1/b-round-1/inputs.sha256`, SHA-256 `004777564cdc29985e7694205c0dcfefc8758384f28d7baaf674a6bf8a759b3f`. It covers the 8 changed files, the frozen diff (`candidate.diff`, SHA-256 `63cbc8a7040837d27668f1da6c3eb7b33c82396ac029b78c27e79a1ea3345825`), the verify logs and the release `roqa.exe`. This packet was written after the freeze and is not in it.
- **Superseded material.** `artifacts/m4-media-paint-1/round-1/` and `verify-r0/` belong to the earlier worker's direction-A state (280-wide pill, glyph bars, fixed 72-DIP clock). They are not inputs to this candidate. Its review's still-applicable findings were fixed here; see "Earlier review findings".

## What was built, against the plan and direction B

Plan: `docs/development/m4-media-preparation.md`, "Bounded implementation sequence" item 2, with the DECIDED open questions. Design: direction B of the M4 mockups, chosen by the user on 2026-11-10.

- **When.** The media notch draws while `media::View::Track` exists, playing or paused. No player, permission and error views draw the unchanged 150 × 30 clock notch (`View::shown`).
- **Pill, 262 × 30** (`layout::MEDIA_NOTCH`). A 20-DIP rounded cover slot at x 10, the title from x 38 with a static end-ellipsis, then the existing clock at its **measured** width, 14 from the right.
- **Panel, 360 × 120, radius 24, ears 16** (`layout::MEDIA_EXPANDED`). A 72-DIP cover at (16, 14); the title, artist and clock on the right. The progress, controls-plus-visualizer and volume rows (below 50 DIP) are reserved and empty. No fake controls and no static progress bar: a snapshot bar would go stale without a tick.
- **Cover slot without art.** Slice 5 brings art. Until then the slot is a neutral rounded rect (white 20 %) with the play-state glyph centred in it: a triangle while playing, two bars while paused. The slot geometry is final, so art will not shift the layout. The mockup's corner badge belongs to the art layout and is not drawn yet.
- **Motion.** Every element moves from its pill position to its panel position by `smooth(p)`, using the accepted springs. Expand and collapse animate to the media sizes. The pill row stays centred during the close undershoot. When a session appears or ends, the shape swaps in one paint, with no tween (documented as provisional).
- **Clock column.** `text_width` measures the current time text in the current clock font each paint; the title column ends 8 → 6 DIP before it and clamps to empty rather than inverting.
- **RTL.** Titles and artists whose first letter is right-to-left are laid out right to left, starting at the column's right edge. The pill layout itself is not mirrored.
- **Pixels.** The cover slot, glyph square and pause bars snap to whole physical pixels.
- **Repaints** (`media::redraw`, `Media::refresh` → `Desktop::paint_notches`). A visible notch repaints only when session presence, title, artist or play state changes. Timeline and rate updates draw nothing. There are no new timers, deadlines, polls, threads or `live` frames. Hidden (fullscreen) notches are skipped and painted before they are restored. Device-loss `Rebuild` re-places every surface through `place`, which paints the current media view.
- **Input region.** The first paint, which precedes showing the window, sets the region for the plain or media shape. Hover and region use the same media flag and progress as drawing.
- **Window.** The fixed HWND grows from 400 × 124 to **440 × 164** DIP. That is the media panel and ears (392 wide), keeping the plain panel's 40 DIP side and 44 DIP bottom room for overshoot and shadow. The plain notch keeps its on-screen position at 100, 150 and 200 % (unit-tested).
- **Text layouts.** Not cached. Static paints happen at most once a minute, and animation frames change font sizes, so a cache would miss. One extra layout per paint measures the clock.
- **Comment hygiene.** Two touched comments that cited Nusa were rephrased to describe ROQA's own behaviour.

## Design addendum

`docs/design/roqa-visual-spec.md`, section "Media notch: direction B, "Cover accent" (user choice 2026-11-10), PROVISIONAL". It records the pill and panel tokens, the per-element pill → panel geometry, type roles, truncation and direction, repaint rules, the window growth and the surface cost estimate. The "Window" bullet above it records the 440 × 164 HWND. The plan's slice-2 "Collapsed look" line now points to the decision and the addendum.

**Process decision for the Chief Thread.** The plan asks for the addendum to be reviewed separately before the paint. Here it ships in the same candidate and was reviewed with it. Whether that review satisfies the separate step is the Chief's call.

## Verification

- **`scripts/verify.ps1`** (`-EvidenceDirectory artifacts/m4-media-paint-1/verify`): checks PASS (fmt, clippy `-D warnings`, tests) and release build PASS. **55 tests passed**, 0 failed.
- **Release `roqa.exe`.** 664,576 B, SHA-256 `25193D312AD5DF23435E23EDB1063A67B1DCB88D46D1CD89D68DFA90407EE3DD`. Against the stated base 656,384 B: **+8,192 B (+1.25 %)**. The base size was not rebuilt in this candidate.
- **New or changed tests.**
  - Scenes from views: no player (all three no-media views draw `NOTCH`), playing pill, paused, expanded panel, empty artist and pending title.
  - Long-title one-line trim within the column; RTL reading direction from the column's right edge; first-letter direction rule.
  - Measured clock column: English, Korean, German, Tamil and Malayalam afternoon formats plus an impossible clock, at 100/150/200 % and three progresses. No inverted rects; the impossible clock leaves an empty title column.
  - Rows scale and stay inside the body at 100/150/200 %.
  - Repaint-only-on-change predicate (`media::redraw`).
  - Geometry: the grown HWND and media pill/panel sizes at 100/150/200 %, body and ears inside the HWND through undershoot, swell and overshoot, unchanged plain-notch screen position, and silhouette region/hit tests for both shapes.

**Surface cost (estimate, not measured).** BGRA at 4 bytes per pixel, per surface: 100 % 288,640 B (+90,240), 150 % 649,440 B (+203,040), 200 % 1,154,560 B (+360,960). Each monitor has a notch and a shadow surface of this size, so double the change per monitor. Actual DComp allocations were not measured.

## Review

- **Reviewer.** Claude Opus 5.5 High, fresh delegated task, read-only, not an author. Sol was not used (budget).
- **Report.** `artifacts/m4-media-paint-1/b-round-1/review-opus-1.md`, SHA-256 `7d863d9ed0b032871e9558097c68aa2cc213f878a48c5fda5fdd5ac028b1205a`. Review thread: `thread:delegated-task:command%3Amcp%3A03b1ee6f-55d8-4bed-be0a-d8f79bfbad14%3Adelegate-task%3Ac0ba009a-32fb-45c1-b6f2-42021d4a9a15`.
- **Inputs.** All 12 manifest entries matched before and after its checks, and `git diff 478d9e0` matched `candidate.diff`. Its own `scripts/check.ps1` run passed with 55 tests.
- **Verdict: APPROVE WITH NITS.** No blocking or major findings. The approval covers code, docs and scripts only; the slice's native evidence is still required.

| ID | Finding | Disposition |
|---|---|---|
| M4B1-OPUS-MIN-1 | `Assert-NoMedia` guards only the region checks, not the pixel and click checks that also assume the plain shape, and cannot tell "no player" from "not logged yet", because ROQA logs nothing for an initial no-player state. A background player gives a misleading smoke failure; never a false pass. | Deferred to the native run: the Chief Thread's desktop-free confirmation must also confirm no media session is open. A robust guard needs an initial-state log line, which is a product change for a later candidate. |
| M4B1-OPUS-NIT-1 | GSMTC "Changing" and "Opened" count as paused, so a track change may briefly show the paused glyph and dim title, with two event-driven repaints. | Accepted; whether real players report "Changing" is UNVERIFIED and part of the native plan. |
| M4B1-OPUS-NIT-2 | On a 1280-px screen at 150 % with 225 % text size, the HWND is capped to the screen width and cuts each media panel ear by about 22 px; the plain panel fits. | Accepted as an extreme configuration; recorded here. |
| M4B1-OPUS-NIT-3 | The one-line assertion cannot fail with wrapping off, and no test checks that a long RTL title's ellipsis falls on the left. | Accepted; the RTL ellipsis side is in the native evidence plan and stays UNVERIFIED. |
| M4B1-OPUS-NIT-4 | The "drawn" fields are listed both in `media::redraw` and in what `media_plan` reads; drift would silently break the zero-repaint rule. | Accepted; any slice that draws a new field (progress in slice 4) must extend `redraw` and its test. |

No fix was applied after the review, so the approval stands on the frozen inputs. There was one review round.

## Earlier review findings (direction-A work-in-progress)

The earlier worker's round-1 review (`artifacts/m4-media-paint-1/round-1/review-opus-1.md`) was on superseded code. Its findings that still applied were fixed before this candidate was frozen:

- **MAJ-1.** `smoke.ps1`'s synthetic `WM_DPICHANGED` check still expected 400 DIP; now 440.
- **MIN-1.** The smoke region checks now throw INTERFERENCE when the log shows a playing or paused session (refined by M4B1-OPUS-MIN-1 above).
- **MIN-2.** The fixed 72-DIP clock column is gone; the clock is measured.
- **NIT-1.** The pill row stays centred through the close undershoot.
- **NIT-2.** Cover slot, glyph and bars snap to whole pixels.
- **NIT-4.** The truncation test was renamed to what it proves.
- **NIT-5.** The smoke side-shadow probes moved to 50 DIP, 10 beside the plain panel, matching the unit test.
- **NIT-6.** The plan's slice-2 "Collapsed look" line was updated.

## UNVERIFIED and NOT RUN

- All native and GUI behaviour: screen appearance of the pill and panel, animation, glyph and text rendering, RTL ellipsis side, input regions for the media shape, fullscreen hiding, rebuild and reposition with media. NOT RUN by rule.
- Collapsed static paint counts across a minute boundary (media adds none, by code and unit test only).
- Real DComp surface memory, private bytes and CPU of the larger surfaces.
- The 656,384 B base size, as stated by the brief; not rebuilt here.
- Smoke, topology, idle and the other suites against this build. NOT RUN.
- Deferred: council review, accessibility review, Windows 10, DWM/GPU/power, native M3 and M4 cases, and the 15-minute trace.
- The separate addendum review (see the process decision above).

## Native evidence plan

Each run needs a fresh desktop-free confirmation through the Chief Thread first, and for smoke, confirmation that no media session is open.

1. **Collapsed and expanded playback capture.** Start a player (for example a browser tab), run this build and capture the pill and panel while playing and paused, at 100 % and, if available, 150 % or 200 %.
2. **Collapsed static paint counts.** With playback static and the notch collapsed, count notch paints across at least one minute boundary. Expected: only the clock's single per-minute repaint, none from media.
3. **Long and RTL titles.** A long Latin title and an Arabic or Hebrew title: one line, ellipsis at the logical end (right for Latin, left for RTL), cover and clock unmoved.
4. **Track changes.** Next/previous in the player: one repaint per drawn change; note whether a brief paused state appears (M4B1-OPUS-NIT-1).
5. **No-player regression.** With no player open, the notch matches the accepted 150 × 30 pill and 320 × 80 panel, with the same screen position as `478d9e0`.
6. **Rerun the five suites** (smoke, topology, idle and the rest of the accepted set) against this build and record the results.

**Native evidence already collected (slice 1).** On 2026-11-10 the user ran `478d9e0`. The log showed `Media: RequestAsync succeeded` and real `playing session=1` transitions (title length 97, artist 18, end 243 s, `callback_on_host=false`). So unpackaged GSMTC access works, and callbacks arrive off the host thread. That run predates this candidate's paint.
