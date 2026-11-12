# F5 features (2026-11-12)

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
