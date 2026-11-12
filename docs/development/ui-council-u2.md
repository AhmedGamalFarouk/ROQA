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
