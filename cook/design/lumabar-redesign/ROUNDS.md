# Rounds (this pass only; the shared ledger `cook/ROUNDS.md` is untouched)

## Reference notes (LumaBar widgets and docs, read 2026-11-24)
- Each widget states one glance fact: "Your next event and how soon it starts", "Temperature now, with today's high and low", AI limits "with when each limit resets".
- Detail lives in separate tiles or a card opened by a click; hover is never described. No live-activity, priority or dismissal rules are documented.
- Taken: one glance fact, click for detail. Not taken: many tiles, compact/mini copies of one widget, widget picker, combined dock.

| Round | Stills | Measured | Judgment (designer, not approval) | Fixed next |
|---|---|---|---|---|
| r01 | `r01-*.png` | "Maghrib in 12 min" 97.4 DIP > 94 budget with a glyph | glyph icons rendered as solid blocks; line clipped; alternative B overlapped the date | dropped the glyph, stroke by attribute, narrowed B |
| r02 | `r02-*.png` | longest line 110.8 of 114 DIP; contrast at rest 6.3:1 (Light) to 10.0:1 (Classic) | reads as today's page with a useful sentence; no stray boxes; Light hover capsule clean | rain copy switched to the existing `rain_soon` string |
| r03 | `r03-*.png` | rail widths from layout constants | rail hover option leaked into the glance hover; active capsule wider than 86 | fixed both, dropped an empty press cell |
| r04 | `r04-*.png` | 6 tabs 232, 7 tabs 258, 2 tabs 128 DIP | glyph rail legible in all themes, Light at ink .55 | none open |
