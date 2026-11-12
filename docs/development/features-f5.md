# F5 features (2026-11-12)

> K1 (2026-11-13): keyboard and UIA/screen-reader support was removed by user decision. Keyboard, Tab, focus-ring, access-key and UIA lines below are history, not current behaviour.

Branch `feat/f5-features` from main 4a93cad. Size budget: baseline 1,121,280 B, limit +2% = 1,143,705 B; never raised.

## P1 Monitor map (Settings > General > Displays)
- `Monitor` gains `rect` (whole monitor); pure `topology::fit(rects, w, h, gap)` scales the real arrangement into the card (2 tests).
- Each monitor is still one native checkbox (state, Tab, Space, UIA) now placed on the map and custom-drawn as a tile; WS_GROUP so arrows move between tiles only; Enter toggles a focused tile. Name: "Display 1, primary, 1920 by 1080 at 125%, ROQA on" (the flag covers notch, dock and edge strips; primary = origin 0,0).
- Same `off_monitors` store key, same `keep_one_monitor` rule; live update on topology change through `refresh_monitors` (now compares size/place too). Old checkbox rows removed.
- Size 1,121,280 -> 1,125,376 B (+4,096). 362 tests.
- NOT RUN: drawing, click/Space/Enter/arrows, Narrator, multi-monitor, DPI, RTL; map height is fixed 120 DIP (many stacked monitors get small tiles).

## P2 Dock slide-down on hide
- `Surface::set_visible(false)` on a visible dock starts a 180 ms slide down (`leaving`); the window hides in `animate` once the slide stops. Showing again mid-slide turns it around. Region, hit-testing and UIA follow the existing `slide` offset; no new timer (the frame loop already runs while `slide` runs).
- Covers the full-screen hide path. Settings turning the dock off still removes it at once (surface released), no animation.
- Size 1,125,376 -> 1,125,888 B (+512). 362 tests (the slide tween test from A1 covers the retarget to 0).
- NOT RUN: native (mid-slide frames, click during slide-down, fullscreen enter/exit).

## P3 Water reminder (Settings > Notch > Water, off by default)
- New `water.rs` (pure): `Config` [goal glasses, glass ml, first hour, last hour, interval min], `clamp`, `wait_secs` (slots from the first hour every interval up to and including the last hour; after the final slot, tomorrow's first, across midnight), `today`, `text`. 4 tests; DST is not handled (local clock seconds; a jump inside the hours shifts that day's slots, the next pass after midnight is right).
- Store keys: settings `water=0|1`, `wcfg=goal,ml,from,to,gap`; progress `wdrank=<day number>,<count>` in the store (a state, not a setting, so the Settings window cannot overwrite it); the count is zero on any other day, so the midnight reset needs no timer. Old files load unchanged (defaults: off, 8, 250, 9, 21, 60).
- Host: `refresh_water` keeps one `water_due` deadline in the existing wait minimum (INFINITE while off); a due slot shows the notch notice "Time for water: 3 of 8 glasses (750 ml)" through the new shared `show_notice` (extracted from `fire_prayer`) and the shared `chime()` (gated by Reminder sounds); silent once the goal is met. The deadline is recomputed after any settings change and on a clock change. "I drank a glass" is a Settings button; progress shows beside it.
- Size 1,125,888 -> 1,131,520 B (+5,632). 366 tests.
- NOT RUN: native (notice, Settings rows, button, midnight). Not built: progress inside the Timer tab panel, an "I drank" action on the notch itself.

## P4 Pomodoro (Settings > Notch > Pomodoro; runs in the Timer tab)
- New `pomo.rs` (pure): `Config` [focus, short, long minutes, sessions per round] (default 25/5/15/4, clamped), `Phase`, `Pomo::advance` (focus -> short break, or long after every Nth session; break -> focus). 2 tests.
- Each phase is an ordinary countdown in `timer.rs` with a `tag` (phase name): same deadline machinery, store expiry, notch pill/panel and UIA name ("Focus, 24:10 remaining"), completion alert and chime (`timer_beeps`, so the Reminder sounds gate applies). `take_pomo_done`, `stop_pomo` (pause keeps the remainder), `start_as` added; 1 test. A tagged timer restored after a restart is a plain timer.
- Host: when a tagged phase ends the next starts by itself with a notice ("Focus done. Short break: 5 min."); Settings buttons "Start or pause pomodoro" and "Skip to the next phase" (needs the Timer tab on). Keys `pcfg=25,5,15,4`; old files load the defaults.
- Size 1,131,520 -> 1,137,152 B (+5,632). 369 tests.
- NOT RUN: native. Not built: start/pause/skip controls on the notch itself (Settings only); the round position is not stored across a restart.

## P5 skipped
Segmented control, stepper and theme tiles not started: ~6.5 KB of the 22 KB budget was left after P4 (limit 1,143,705 B), too little for three owner-drawn control kinds without tripping the ratchet. Native drop-downs stay.
