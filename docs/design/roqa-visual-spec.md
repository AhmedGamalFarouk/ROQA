# ROQA visual spec: notch and dock (design pass 1)

Author: `/root`, 2026-11-09. Reference study: `nusa-reference.md`. All sizes are in DIPs, scaled per monitor DPI; settled edges are snapped to whole pixels. Colours are sRGB.

## Surfaces and theme

- **Always dark by default (Classic theme; other themes in `roqa-themes.md`).** Both surfaces use a black body with white content, like Nusa and hardware notches, and follow neither light nor dark mode: a black notch is part of the screen edge, not app chrome.
- **High contrast.** System high-contrast colour mapping isn't implemented yet; see the limits.

## Notch (`src/layout.rs`, `src/graphics.rs`, `src/host.rs`)

| Token | Collapsed pill | Expanded panel |
|---|---|---|
| Body width × height | 150 × 30 | 320 × 80 |
| Bottom corner radius | 14 | 24 |
| Concave top ear | 10 | 16 |
| Fill | `#000`, opaque | `#000`, opaque |
| Shadow | none | Body inset 6 at the sides, ending 7 above its bottom; Direct2D shadow σ 20, black at 0.9 × open progress |

### Window, hit area and region

- **Window.** A fixed **440 × 164** HWND, centred and flush with the work-area top. It holds the larger media panel (below) plus ears, room for the open overshoot, and the shadow below and beside the body, keeping the plain panel's 40 DIP of side and 44 DIP of bottom room. It never moves or resizes. It was 400 × 124 before the media panel; the plain notch's on-screen position is unchanged wherever the 40-DIP growth scales to an even number of pixels, which includes 100, 150 and 200 %.
- **Body region (`layout::notch_rows`).** Updated with the same progress used to draw each frame, with no speculative growth lead. One-pixel rows follow the circular concave ears and rounded bottom corners, using the same rounded body and radius clamp as drawing. Rows conservatively contain edge coverage within a one-pixel fringe.
- **Shadow.** A separate layered, transparent, nonactivating HWND sits directly behind the body. It follows the same bounds and animation; input passes through it to other processes. Canvas and window ownership follow the notch lifetime.
- **Hover (`layout::notch_hit`).** Uses the same silhouette edge with 4 DIP of slop, rather than a rectangular ear band.
- **Shared geometry.** Drawing, hover and region share `layout::notch_body` and `notch_radius`; the region and hover also use the same analytic circular edge.
- **Text size.** The Windows text-size factor (Accessibility > Text size, `UISettings.TextScaleFactor`) multiplies the notch's effective DPI. The whole notch, whose only content is text, scales and is rebuilt live on the change event.

### Type

The font is Segoe UI Variable, from the system; no font is bundled.

| Role | Size | Weight | Colour |
|---|---|---|---|
| Clock (one element, pill to panel) | 15 → 30 (Text below 20, Display above), am/pm at 0.72× | Bold | white |
| Expanded date (long, user locale) | 13 (Text) | Regular | white 60 % (≈ 7.4 : 1 on black, AAA) |

- **Designator.** The am/pm designator is whatever text precedes the first or follows the last digit in any script, so the user-locale format is never re-ordered.
- **Overflow.** Text is centred and clipped to the body; long dates end in an ellipsis.

### Media notch: direction B, "Cover accent" (user choice 2026-11-10), PROVISIONAL

**PROVISIONAL.** The user chose direction B from the four M4 mockups (`artifacts/m4-mockups/notch-variants.html`, section B, outside version control) on 2026-11-10. This section records B's geometry as slice 2 draws it. It reuses the tokens above and has had no separate design review; it is expected to change. The no-player notch above is unchanged.

- **When.** The media notch is drawn while a media session exists (`media::View::Track`), playing or paused, even before its title arrives. No player, refused access and errors all draw the plain 150 × 30 clock notch. Refused access and errors add one status line to the open plain panel (below).
- **Shape changes are instant.** When a session appears or ends, the shape swaps in one paint, with no tween. Expanding and collapsing animate with the accepted springs to the media sizes. Play, pause and track changes never change the geometry.

| Token | Media pill | Media panel |
|---|---|---|
| Body width × height | 262 × 30 | 360 × 120 |
| Bottom radius, ears | 14, 10 | 24, 16 |
| Fill, shadow | as the plain notch | as the plain notch |
| Hover swell (progress 0.07) | about 269 × 36 | — |

**Layout.** Values are DIPs from the body's left and top edges, or from its right edge where stated. Every element moves from its pill value to its panel value by `smooth(p)`. In the pill, rows are centred on the body's middle (15, or less during the close spring's undershoot).

| Element | Pill | Panel |
|---|---|---|
| Cover slot | 20 × 20 at x 10, centred; radius 5 | 72 × 72 at (16, 14); radius 10 |
| Play-state glyph | 10, centred in the slot | 24, centred in the slot |
| Title | from x 38, ending 8 before the clock | from x 100, centre y 21, ending 6 before the clock |
| Clock | trailing edge 14 from the right | trailing edge 16 from the right, centre y 19 |
| Artist | not drawn | under the title column, centre y 38 |

- **Cover slot.** Album art (slice 5) fills exactly this slot, clipped to its rounded rect, so nothing moves when it arrives. It is a centred square crop, at 60 % opacity while paused like the glyph. Whenever a track has no art, the slot is a neutral rounded rect (white 20 %) with the play-state glyph centred in it. The glyph is drawn on a 10-unit grid: a triangle (2.5, 1)–(9, 5)–(2.5, 9) while playing, and two 2.2 × 8 bars at x 2 and 5.8 while paused. The mockup's corner badge belongs to the art layout: with art, the pill draws a black 11 circle 4 past the cover's right and bottom edges, carrying the play-state glyph at 6. It fades out as the panel opens, whose play/pause control shows the state, and is never drawn without art.
- **Clock column.** Its width is measured with DirectWrite for the current time text, font and scale, never a fixed column, because long locale designators are much wider than "9:50 pm". The title column ends before it. If a clock is wider than the whole row, the title column becomes empty rather than inverted.
- **Artist.** It fades in by `smooth((p − 0.35) / 0.45)`, on the date's schedule, while settling 6 upward into place. An empty artist or title is skipped.
- **Controls row (slice 3, PROVISIONAL).** Previous, play/pause and next, at the mockup's direction-B places, in DIPs from the body's left and top (`layout::media_controls`):

  | Control | Box | Look |
  |---|---|---|
  | Previous | 28 × 32 at (100, 66) | 14 DIP white glyph: bar (1.5–3.1) and left triangle (9, 1)–(9, 9)–(3.8, 5) |
  | Play/pause | 32 × 32 at (148, 66) | White circle with a 12 DIP black glyph: the action, so pause bars while playing and the play triangle otherwise |
  | Next | 28 × 32 at (200, 66) | 14 DIP white glyph: right triangle (1, 1)–(6.2, 5)–(1, 9) and bar (7–8.6) |

  - The row keeps its panel place while the body morphs: the body clip reveals it, and it fades in on the artist's schedule (`motion::reveal`). The pill draws no controls.
  - A control the player does not enable (GSMTC `IsPreviousEnabled`; `IsPauseEnabled` while playing, `IsPlayEnabled` otherwise; `IsNextEnabled`) is drawn at 30 % opacity, never hidden.
  - The control with keyboard focus gets a white ring: 2 DIP stroke, 2 DIP outside the box, a circle around play/pause and radius 8 + 3 otherwise.
  - There is no pointer hover or press state: a control changes only when the player reports the result.
- **Progress row (slice 4, PROVISIONAL).** The addendum reserved 50–64 but did not pin the geometry; this minimal placement has had no design review. In DIPs from the body's left and top (`layout::media_progress`), centred on y 57:

  | Element | Box | Look |
  |---|---|---|
  | Elapsed | 100–136, leading | `m:ss`, 11 Regular, white 60 % |
  | Bar | target 164 × 14 at (140, 50); drawn 4 tall, centred | Track white 20 %, filled part white, radius 2; the fill is whole physical pixels, floor(elapsed × width ÷ length) |
  | Total | 308–344, trailing (16 from the right, like the clock) | `m:ss`, 11 Regular, white 60 % |

  - It keeps its panel place while the body morphs and fades in with the controls (`motion::reveal`). The pill draws none.
  - Drawn only while the player reports a length (`EndTime` after `StartTime`, at most 1000 hours); without one there is no row and no seek. Times are whole seconds, floored; a track over an hour reads, for example, `62:05`.
  - The shown position is `Position + (now − LastUpdatedTime) × rate`, clamped to the timeline. It holds still while paused.
  - **Seek.** A click or tap inside the bar's target, when the player enables `IsPlaybackPositionEnabled`, asks for that fraction of the length; the left edge is the start. There is no hover, drag or keyboard seek (user decision, 2026-11-10). When the player accepts it, the timeline is read again; if that read still predates the request, the row shows the target until the player reports a newer timeline.
- **Volume row (slice 6, PROVISIONAL).** The default render endpoint's master volume (user decision, 2026-11-10: overall level, not per app), in DIPs from the body's left and top (`layout::media_controls`, `layout::volume_bar`):

  | Element | Box | Look |
  |---|---|---|
  | Mute toggle | 24 × 16 at (100, 100), the fourth control | 14 DIP white speaker glyph: box (1–3.5, 3.5–6.5), cone (0.5, 5)–(6.5, 1)–(6.5, 9), and two wave bars (7.6–8.4, 3.5–6.5) and (9–9.8, 2–8). Muted: the speaker without the waves |
  | Level bar | hit box 212 × 16 at (132, 100), ending 16 before the body's right edge | A 4 DIP track centred in it (white 40 %, about 3.7 : 1 on black, fully rounded; brighter than the progress track's white 20 % for target contrast, which design review may align), filled in white from its left end to the shown whole percent |

  - It is revealed and placed like the controls row: fixed panel place, body clip, `motion::reveal`. The pill draws no volume.
  - A click or tap on the bar sets the level: its first pixel is 0 and its last 100 %. While muted, that click also unmutes, so the toggle then reads "Mute, volume N%" (user decision, 2026-11-11; slice 7). A click on the toggle flips mute. All are ROQA's own changes, tagged with ROQA's event context, so their echo wakes nothing; ROQA re-reads the endpoint once instead. Level changes made outside ROQA never change mute.
  - The level is shown as the whole percent Windows uses; the fill and the accessible name are drawn from it alone. While muted, the fill drops to white 60 % (about 3.2 : 1 against the track) and the toggle stays fully opaque, because it is still a live control.
  - With no default render endpoint (none present, or the last one unplugged), the toggle and fill are drawn at 30 % with an empty fill, the toggle is disabled, and nothing polls: the row returns on the next device-added, device-state or default-changed notification.
  - The bar is not mirrored for an RTL UI language (deferred with the rest of the layout mirroring). There is no drag, hover or press state.
- **Visualizer (slice 7, PROVISIONAL).** The addendum reserved the slot right of the controls but did not pin it; this minimal placement has had no design review. In DIPs from the body's left and top (`layout::meter`): five bars, each 4 wide and fully rounded, spread across 312–344 (16 from the right, like the progress total and the volume bar) and bottom-aligned at y 92 in a 20 tall slot (72–92), centred on the controls row. White 60 %, like the artist.
  - Each bar is one sample of the default render endpoint's peak level (user decision, 2026-11-10: the whole endpoint, all apps mixed, measured before the volume, so a muted endpoint still moves), oldest on the left. It is not a spectrum. Its height is 4 at silence (a dot) up to 20, in whole physical pixels: 4 + round(peak × 16) at 100 %.
  - It is revealed and placed like the controls row: fixed panel place, body clip, `motion::reveal`. The pill draws none.
  - It samples only while a visible notch is open or opening and the track plays. Paused, the bars lie flat; collapsed, hidden, full-screen-suppressed, or with no player, endpoint or meter, nothing is sampled. Bars restart flat, never from old samples.
- **Timer (M5 S5, PROVISIONAL).** The soonest countdown timer (up to 3 run; `+N` counts the others), in 12 DIP regular, white 85 %, no accent colour, in the cover-accent style. No design review; the geometry is expected to change. In DIPs from the body's left and top (`layout::timer_panel`, `timer_pill`):
  - **Plain panel (320 × 80):** `m:ss` (`h:mm:ss` over an hour), left-aligned in 16, 21, 72 × 16, centred on the clock row (y 29): left of the centred 30 DIP clock (about 100 wide) and above the date row (48–68). Fades in with the date.
  - **Media panel (360 × 120):** the same text in 16, 92, 72 × 16: under the 72 DIP cover (ends y 86) and left of the controls and volume rows (from x 100). A layout test checks that it overlaps no control, the cover, the progress row, the volume bar or the meter at 96 to 240 dpi.
  - **Plain pill (150 × 30):** whole minutes (`5m`, rounded up), right-aligned in 108, 8, 30 × 14, right of the centred 15 DIP clock; it fades out as the panel text fades in. The media pill (262 × 30) is full, so it shows none. It repaints with the clock's minute change only, so it can lag its own minute boundary by up to 59 s.
  - **Completion:** every visible notch opens once and the system sound plays; with no pointer or focus on it, it closes 10 s later. The panel shows `0:00` meanwhile.
  - **UIA:** the root's name carries each timer's state, such as "ROQA Notch, Timer, 4:32 remaining" (several: "Timer 1, 0:45 remaining; Timer 2, …"), and "Timer finished" during the alert. It is read at the last paint, not live.
  - The bars are decorative: UI Automation does not expose them.
- **File shelf (M5 S6, PROVISIONAL).** File references (path and name only; no content is read) as 16 DIP system icons in 24 DIP square slots, the newest `shelf::VISIBLE` (3) drawn left to right, fading in with the panel; a target found missing at activation is dimmed to 30 %. No design review; the geometry is expected to change. In DIPs from the body's left and top (`layout::shelf_slot`):
  - **Plain panel (320 × 80):** slots at x 236, 260, 284 (y 17–41), on the clock row, right of the centred clock (it ends about x 228) with a 12 DIP right margin. **Media panel (360 × 120):** the same x offsets start at 232 (y 70–94): the free pocket right of the controls row (ends x 228), left of the visualizer (from x 312), between the progress row (ends y 64) and the volume row (from y 100). A layout test checks no overlap with the cover, title, clock, controls, progress, volume, meter or timer slots at 96 to 240 dpi.
  - **Input:** left click or UIA Invoke opens the file through the shell, Ctrl+click reveals it in Explorer, right click removes the entry, Ctrl+O while the notch has keyboard focus opens the standard file dialog. A file drag over the notch opens it like a hover; a drop adds the files at the front (12 kept, the oldest evicted).
  - **UIA:** one Button per visible entry after the media controls, "Shelf: notes.txt" plus ", file not found" or ", network location, not checked"; ids `RoqaShelf0` to `RoqaShelf2`; Invoke opens; not keyboard-focusable (focus stays on the media controls). Entries past the third are stored but neither drawn nor exposed until one is removed.
- **Status line (M4 gate fix 1, PROVISIONAL).** `View::Permission` and `View::Error` draw one short line in the open plain 320 × 80 panel, under the date: "Windows blocked media access" or "Media controls unavailable". 11 DIP Segoe UI Variable Text regular, centred, white 60 %, revealed with the date (`motion::reveal`), in a row centred on y 72 (66–78), trimmed with an ellipsis. The pill, the media panel and `NoPlayer` draw none. The notch repaints when the status changes. This geometry has had no design review and may collide with the date row (48–68) at large text scales.
- **Reserved panel rows.** Nothing else is drawn below 50, and there are no placeholders.

**Type.** Segoe UI Variable Text in every role. White on black is 21 : 1; white 60 % is about 7.4 : 1.

| Role | Pill | Panel | Colour |
|---|---|---|---|
| Clock | 15 Bold, am/pm at 0.72× | 12 Regular, am/pm at 0.72× | white → white 60 % |
| Title | 12 Regular | 13 SemiBold | white 85 % → white |
| Artist | — | 12 Regular | white 60 % |

Weights switch at the midpoint of the morph. While paused, the glyph and the title are drawn at 60 % of their opacity.

**Truncation and direction.**

- **One line each, no wrap.** Titles and artists end in a static character-granularity ellipsis. There is no marquee or scrolling, because those need a continuous repaint.
- **Reading direction.** A title or artist whose first letter is from a right-to-left script (Hebrew, Arabic and others) is laid out right to left. It starts at its column's right edge, and its ellipsis falls on its logical end, the left.
  - The layout itself is not mirrored: the cover stays on the left and the clock on the right.
  - Mirroring for an RTL UI language is deferred.

**Repaints.** A visible notch repaints only when the drawn track changes: session presence, title, artist, play state or the controls' enabled states (`media::redraw`). A volume or mute change repaints only visible notches whose panel is open or opening, and only when the whole percent or mute state changes. Moving keyboard focus paints one frame for the ring. A collapsed notch draws no timeline, so timeline and rate updates draw nothing there, and there is no media timer, deadline or frame loop for it. Album art adds one paint when it arrives. Collapsed static playback otherwise keeps only the clock's per-minute paint. Text layouts are not cached: static paints happen at most once a minute collapsed and about once a second open, up to 15 a second only while the visualizer runs, and animation frames change the font sizes, so a cache would miss.

- **Open panel (slice 4).** An open, visible notch repaints when its progress row's shown seconds or filled pixels change. While the track plays forward, one host deadline falls at the next whole second or bar pixel, whichever is first. At rate 1 or below a whole second is never deferred, so the time neither lags nor skips; a pixel-only change waits until a second after the previous tick's scheduled time (not its late wake, so latency never accumulates) and is drawn on its own only if the next second is still a second later. Above rate 1 every change waits for that floor. Ticks are therefore at least a second apart after the first one. A seek or other read repaints at once. Paused, rate 0, no length (or one over 1000 hours), collapsed, hidden or full-screen-suppressed: no deadline.
- **Visualizer (slice 7).** While it runs, one host deadline falls at least 67 ms after the previous sample (at most 15 a second, even after a late wake), with no compositor clock. Each sample is one `IAudioMeterInformation::GetPeakValue` call. An open, settled notch repaints only when a bar's whole-pixel height changes, so silence and a steady level cost samples but no paints; while it animates, its frames already paint. Stopped, it has no deadline and makes no meter call.
- **Timeline reads.** There is no timeline event subscription. The timeline is read with the playback info, when a notch opens, and while one is open after a track change or a finished command such as a seek.

**Window.** The fixed HWND grows from 400 × 124 to 440 × 164. That is the media panel and its ears (392 wide) with the plain panel's 40 DIP from body to window side and 44 DIP of bottom room, for the open overshoot and the shadow.

**Cost of the larger window.** These are estimates of DirectComposition surface pixels, BGRA at 4 bytes per pixel; actual allocations were not measured.

| Scale | Each surface, 440 × 164 DIP (was 400 × 124) | Change | Per monitor: notch plus shadow surface |
|---|---|---|---|
| 100 % | 440 × 164 px, 288,640 B (was 198,400) | +90,240 B | +180,480 B |
| 150 % | 660 × 246 px, 649,440 B (was 446,400) | +203,040 B | +406,080 B |
| 200 % | 880 × 328 px, 1,154,560 B (was 793,600) | +360,960 B | +721,920 B |

## Notch motion (`src/motion.rs`), fitted to the live Nusa capture

| Phase | Token |
|---|---|
| Hover intent | On entering the hover band, the pill swells to progress 0.07 (about 162 × 33) over 320 ms (ease-out `cubic(0.2,0.9,0.3,1)`). It opens only if the pointer is still there. Re-entering while still closing reopens directly |
| Open | Damped spring, ζ 0.65, ω 18 rad/s, about 7 % overshoot peaking at about 230 ms. Bounded at 500 ms, then it lands exactly on target |
| Close | Damped spring, ζ 0.61, ω 16 rad/s. Reaches the pill at about 190 ms, undershoots about 9 %, and is bounded at 560 ms |
| Size | Width and height follow the spring, including overshoot and undershoot. Radius, ears and shadow follow the progress clamped to 0–1 |
| Content | One clock morphs: its size goes 15 → 30 and its centre moves from the pill middle (15) to the time row (29), both by `smooth(p)`. It never fades, so there is no gap. The date fades in by `smooth((p − 0.35) / 0.45)`, sliding 6 down into place |
| Interruption | Every re-target starts from the current value; velocity is not carried over |
| Reduced motion | None. By user decision (2026-11-09) ROQA always animates, like Nusa, whatever the Windows "Animation effects" setting says |

## Dock (`src/dock.rs` geometry unchanged; `src/graphics.rs` drawing)

| Token | Value |
|---|---|
| Bar | Height 64, padding 8, slot 56, icon 40. Unchanged, so hit-testing and UIA rectangles are identical |
| Bar shape | Anti-aliased rounded rectangle, radius 20. The corners are transparent; there's no window region |
| Bar fill | Idle `#000`; pointer inside `rgb(20,20,22)`. This keeps M1's hover semantic |
| Edge | One-physical-pixel inner hairline, white 10 % |
| Slot hover | A 48 × 48 chip, white 12 %, radius 12, centred on the icon and lifting with it |
| Icon lift | 3, snapped to whole pixels so shell bitmaps never resample |
| Active window | White 92 % pill, 12 × 3, 4 above the bar bottom |
| Picker glyph | 2 × 2 tiles of 12, gap 4, radius 3, `rgb(220,225,232)` |
| Motion | Hover in over 160 ms, out over 180 ms, ease-out `cubic(0.2,0.9,0.3,1)`. Highlight, lift and bar fill move together |

## Frame pacing and idle cost

- **Painting.** A surface paints per frame only while a tween runs or hover intent is pending, then one final exact frame.
- **Waiting while animating.** The loop waits on `DCompositionWaitForCompositorClock` (the same IPC handles, timeout ≤ 33 ms), then drains input, so input waits at most one frame. That function is Windows 11 only, so it is resolved at run time with `GetProcAddress` and ROQA still loads elsewhere. When it is absent, fails, returns display occlusion or an unexpected status, the loop falls back to a ≤ 16 ms message wait.
- **Idle.** The original blocking `MsgWaitForMultipleObjectsEx`, with only the catalog and minute deadlines. No timer and no polling.

## Accessibility

- **Dock.** UIA rectangles and hit-testing are unchanged; the picker remains the keyboard and Narrator route.
- **Notch media controls (slice 3).** A ToolBar root ("ROQA Notch"; "ROQA Notch, Windows blocked media access" or "…, Media controls unavailable" while the status line shows, PROVISIONAL) with three Buttons while a session exists: "Previous track", "Pause, Title – Artist" or "Play, …", and "Next track". Slice 6 (PROVISIONAL) adds a fourth Button, the mute toggle: "Mute, volume 40%", or "Unmute, volume 40%" while muted, and "Volume unavailable, no audio device", disabled, without an endpoint. The level is read-only text in that name; there is no RangeValue and no arrow-key adjustment. Invoke works whether or not the panel is open; a disabled control reports `IsEnabled` false and refuses Invoke and focus. Bounds are the open panel's while it is open or opening, and offscreen otherwise.
  - **Keyboard (user decision, 2026-11-10).** Entry is only through UIA focus, such as Narrator moving to a control: the notch takes the foreground if Windows allows it and opens. Tab and Shift+Tab, and the arrow keys, move among enabled controls, wrapping; Enter and Space press the focused one; Escape closes the panel and gives the foreground back to the window that had it. No global hotkey, IPC command or picker entry.
  - Pointer use stays nonactivating: a click on a control never takes focus.
  - **Position (slice 4).** While the progress row is drawn, the play/pause Button's `ItemStatus` is its text, such as "1:23 of 4:56"; otherwise it is empty. It is read-only: no RangeValue and no arrow-key seek (user decision). No property-changed event is raised as it advances.
- **Contrast.** The time (white) is 21 : 1 and the date (60 % white) about 7.4 : 1 on black.
- **Pointer targets.** The collapsed notch hit area equals its visible footprint.
- **Motion.** Reduced motion is deliberately not honoured, by user decision. This is a known accessibility regression against the original brief, recorded in the ledger.

## Known limits of this pass

- **Text scaling.** The notch follows the Windows text-size setting; this is verified only at 1.0 on this machine, and other factors use the same path as DPI scaling. The dock has no text.
- **High contrast.** There's no high-contrast colour mapping.
- **Input edge.** The pixel region conservatively contains the anti-aliased outline within a one-pixel fringe. Visual edge quality still needs fresh capture after this change.
- **Dock corners.** The dock's transparent anti-aliased corners are inside the HWND and take input on a few pixels.
- **Evidence.** Frame pacing is evidenced by screen-capture sequences (about 140 Hz sampling), not by a compositor frame-time trace.
