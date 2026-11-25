# Notch glance line and rail glyphs (LumaBar-inspired pass, design spec, 2026-11-24)

Owner approved the glance line (A, title stays "Media") on 2026-11-24 and asked for a rail-glyph mock. All three are built and pushed: line `7b7d43c`, hover `df56d60`, rail glyphs `3fc585a`. Not yet seen live on the second monitor.
Brief, reference notes and round log: `cook/design/lumabar-redesign/{BRIEF,ROUNDS}.md`.
Stills (2x, headless Chrome from `lumabar-redesign/mock.html?v=...`), sample data 24 Nov 2026:
| still | shows |
|---|---|
| `lumabar-redesign/r02-compare.png` | live capture now, line A, rejected alternative B |
| `lumabar-redesign/r02-states.png`, `r02-themes.png`, `r02-edges.png` | line A: sources, hover (deferred), click, themes, long strings |
| `lumabar-redesign/r04-rail.png` | rail now (live captures) vs glyph rail: rest, hover, other tab, 7 tabs, 2 tabs |
| `lumabar-redesign/r04-railthemes.png` | glyph rail in the five themes, Light with a hovered tab |
Mock metrics differ from the code: the line is 11 DIP at ink .60 in a 112 DIP rect (the mock drew 12/.70/130). The mock glyphs approximate `tab_art`'s.

## Audit (what is there now)
- Collapsed pill: clock; playing media adds cover and level bars; a running timer shows minutes; notices take over. Already a glance surface; unchanged.
- Open notch, nothing playing: the pill's clock again in large type, date, Hijri, the eyes and "Nothing playing".
- Rail: the active tab is a labelled capsule, the others bare 7 DIP dots, learnt only by hovering.
- Dock right cluster: already a glance icon plus popover per item; unchanged.

## 1. Glance line (built)
When the Media page shows no track, no notice and no media status, the line under the eyes says the next thing from a tab that is on. A click opens that tab.
| # | when | text | click opens |
|---|---|---|---|
| 1 | a countdown runs (not done, not the stopwatch) | "Timer, 18 min left" | Timer |
| 2 | next prayer under an hour away | "Maghrib in 12 min" (`prayer_tab::countdown`) | Prayer |
| 3 | next prayer an hour or more away | "Isha at 7:02 pm" (the page's own row time) | Prayer |
| 4 | Weather on and loaded | "29°, clear" | Weather |
| 5 | none | "Nothing playing" (unchanged) | nothing |
Dropped from the first pass, on inspection: AI limits (only fetched while their popup is open; showing them would add work) and rain (already its own notice, which hides the line anyway). "Maghrib now" dropped: the prayer notice covers that moment.
- Same text slot, font and colour as today's status line. No glyph, no accent.
- Update: the Prayer and Weather page text now also builds while the Media page is selected, on the same minute and data keys as before. No new timer, worker, fetch or storage. The weather fetch policy is unchanged.
- Hit target: 112 x 24 DIP centred on the line (`layout::glance_at`).
- Hover capsule built (`df56d60`); host.rs ceiling raised to 4,918 (owner approved). Was: **deferred.** It needs new pointer state in `host.rs`, which sits at its line ceiling (4,898). Adding it means raising that ceiling (allowed by the 2026-11-23 owner decision) or moving notch hover out first.

Code: `src/tabs.rs` (`glance` + test), `src/layout.rs` (`glance_at`), `src/host/notch_clicks.rs` (click), `src/host/notices.rs` and `src/host.rs` (build the text on the Media page; pass the line to paint).

## 2. Rail glyphs (built, `3fc585a`)
- Each inactive tab shows its existing `tab_art` glyph at 14 DIP in a 16 wide slot, ink .45 (Light .55), never accent. Gap 10 and pad 8 unchanged.
- The active tab keeps its 86 wide labelled capsule and Lamp underline; hover still opens a glyph into its label (pop); press unchanged; one tab still has no rail.
- Widths (computed from the layout constants): six tabs 232 DIP (now 187), seven 258 (now 204), two 128. All fit inside the 360 panel.
- Hit test: each slot widens from 7 to 16, so `rail_tab_at` gets bigger targets for free.
- Implementation size: `layout::rail_width` rest width 7 to 16, and draw the glyph instead of the dot in the rail painter. No new state.

## Measured (a command produced these)
- Line text at 12 px (mock): longest "Maghrib at 12:58 pm" 110.8 DIP. At the real 11 px, about 101.6 DIP, inside the 112 rect.
- Line contrast at the mock's ink .70: 6.3:1 (Light) to 10.0:1 (Classic). At the real ink .60 it is lower; not remeasured.
- `scripts/verify.ps1`: checks (fmt, clippy, tests, file sizes) pass. Its release step could not replace `target/release/roqa.exe` because the owner's ROQA is running from it. A release build in a scratch folder passed: 1,795,584 B, +9,728 B over the running exe, a figure that also includes other threads' uncommitted edits.

## Visual judgment (designer, not approval)
The line reads as today's page with a useful sentence; the glyph rail reads at a glance without hovering and stays quiet next to the Lamp underline. No critic round has run; neither change has been seen live on the second monitor.

## Ranked: what is left
1. Hover capsule for the line (needs the host ceiling decision).
2. Rail glyphs (this mock).
3. Prayer soon in the collapsed pill (busier pill; mock only if wanted).
4. Dock: no change. 5. Avoid: renamed tabs, panel restyle, widget grid, more clock styles, combined dock.
