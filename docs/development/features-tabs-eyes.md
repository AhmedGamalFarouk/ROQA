# Features: notch tabs and mascot eyes

Branch `feat/tabs-eyes` from `a0d78f0`. No native/GUI run. Design: `artifacts/design-mascot/round2.html` (git-ignored).

## Design variants used
- Eyes: **C "Squash"** (Capsule family, card `vC`, `blsq` keyframes: squash 1.3 x 0.1, stretch 0.86 x 1.14). Flat lids, no pupils, no 20 ms lag.
- Tabs: **T3 "Hanging handle"** (card `T3`, class `r-hang`): 70 x 11 dots at rest, 152 x 32 icon buttons open, 240 ms grow, 450 ms collapse delay. The page's combined mock paired E with T1; the user's pick (C, T3) was followed instead.

## What changed
- `tabs.rs` (pure): `Tab::available()` (Weather, Prayer false), ordering, `pick` (choice, else Media if a session lives, else last used), `Handle` (open at once, one 450 ms close deadline).
- Pages: Media = today's media/clock panel; Timer = title, big countdown, clock; Shelf = slots plus hint. Timer and shelf no longer share the media panel's pockets. Left/Right switch tabs (Tab and Up/Down still walk controls). Last tab is kept in the store (`lasttab=`).
- Handle: `layout.rs` geometry, region rows and hover include it; icons are D2D paths in `graphics/tab_art.rs`. UIA: TabItems "Weather tab, 2 of 5" (n of shown), Invoke switches, position/size of set, ItemStatus Selected.
- Settings: a checkbox per tab and "Mascot eyes" (`tab<key>=`, `eyes=`, default on), applied live. A tab off leaves the bar and stops its work: Media (no session draw, meter, progress or media deadline), Timer (no deadline, alert or tick; Start timer says so), Shelf (no icons, drops ignored). One tab left: no handle.
- Eyes (`eyes.rs`, pure): idle, happy (playing), sleepy (10 min idle or 23:00-04:59), surprised (timer done or power notice). Focused skipped; adhan and rain wait for their tabs. Eyes sit in the pill's title lane, so the media title now shows only in the open panel.
- `topology-check.ps1`: `$strips = @(switch ...)`; parse-checked only.

## Render cost
- Blink is never scheduled on its own: only the once-a-minute clock repaint, a mood change or the pointer entering start one. Idle collapsed notch with eyes on: the clock repaint draws the squash pose and one 130 ms deadline draws the rest pose, so **2 paints a minute instead of 1** (eyes off: exactly the old rate, no state, no deadline). A mood change is 3 paints; hover adds none (frames already run).
- Handle paints only while the panel is open and it moves (about 14 frames per grow); no timer beyond the 450 ms close.

## Checks
- verify.ps1 PASS (fmt, clippy -D warnings, 252 tests, locked release). 968,704 B (+5.12% over 921,600), ratchet tripped; baseline raised with `approved-by: user 2026-11-12` (the gate wants the date last; the batch label is in `raise-reason`). SHA `7428929EDBAF6897024B79DEF0B09ECF902D8BB98E08F68027FC4F5B9B0BE96A`.

## NOT RUN / deferred
- Real drawing of the handle, icons, pages, eyes and the layer fade; hover/click/keyboard; UIA read; DPI and text-scale fit; GUI paint counts. Reviews paused (REVIEW NOT RUN).
- Deferred: SelectionItem pattern, Weather/Prayer content, adhan/rain eyes (queue W1, P1, M1, T2).
