# U2 council fixes (batch U2, six packages)

Source: `docs/development/ui-council-fixes.md` (root checkout, untracked) and `artifacts/council-3`. Size ratchet: baseline 1,121,280 B, limit 1,143,705 B; U2 started at 1,137,152 B (6,553 B headroom).

## P1 Settings window
- Footer is its own 52 DIP strip with a divider; page controls are clipped to the view above it with a window region (`clip`), the footer message sits left of Close and never overlaps it.
- Captions, labels and button captions are measured (`DrawTextW` calc) and wrap; the row grows, no half sentences.
- Edits lose `WS_BORDER`; edits and drop-downs take the `DarkMode_CFD` theme in dark mode (no white boxes).
- Accent now read from `HKCU\...\DWM\AccentColor` (what the shell shows), lightened on dark cards when too dark; switch on-state uses it. Sidebar focus is a 2 px accent ring.
- Sidebar tiles draw Segoe MDL2 Assets glyphs (gear, palette, monitor, task view, sun, moon, stopwatch, info), font made per paint and deleted.
- Size 1,137,152 -> 1,140,224 B (+3,072). 369 tests.
- NOT RUN: native/GUI. Unverified by eye: the CFD theme on combos and edits, glyph codepoints, region clipping, footer at 125%/150% scale, light mode.

## P2 Shelf
- Each slot is 96 x 36 DIP: a 20 DIP shell icon with the file name under it (10 pt, centred, ellipsis), the focus ring surrounds the cell; add and more slots are labelled "Add" and "More". The empty-shelf hint moved right of the add slot.
- No icon yet, or the shell had none: a new `File` glyph replaces the blank square.
- Size 1,140,224 -> 1,140,736 B (+512). 369 tests. NOT RUN: native/GUI (icon delivery for txt/png/pdf not confirmed by eye; the council capture showed none arriving).

## P3 Notch
- Prayer: columns 40/44 DIP at 9.5 pt so "Maghrib" fits; the wash marks the prayer the headline names (the next), the current one is only brighter and bolder; chips are 29 DIP tall and the gear sits 2 DIP lower, so they no longer touch.
- Timer: when the last timer finished the panel reads "Timer finished" in the accent colour (`View::done`), for the existing 10 s alert.
- Limits: Refresh pill moved to x 96 and 18 DIP tall (clear of the title and the column labels); column labels 8.5 pt.
- Right-click menu on the notch closes the hover-opened panel, and hover does not open it while the menu is open.
- Size 1,140,736 -> 1,141,248 B (+512). 369 tests.
- Not done (size): Limits adaptive height and bigger body text, timer restart/dismiss buttons. NOT RUN: native/GUI.

## P4 Task panels
- Timer idle reads "Start a 5 min timer" (accent); a click on that line starts 5 min, and on "Timer finished" restarts it (`layout::page_value_at`, shared `on_slot` with Refresh).
- Weather and Prayer: a click on the page's value line (where "Set your city in Settings" shows) opens Settings.
- Media with nothing playing says "Nothing playing" under the date (the media failure line still wins).
- Size 1,141,248 -> 1,141,760 B (+512). 369 tests.
- Not done (size): preset buttons beyond the one 5 min start, a drawn "Set city" button, tab handle labels, bigger handle hit targets, Tab/UIA control for the new click lines. NOT RUN: native/GUI.
