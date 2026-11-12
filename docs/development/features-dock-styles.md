# D1: dock style, Floating or Attached (2026-11-12)

Branch `feat/f3-dock`. No native/GUI run; nothing drawn or hit-tested on a real screen (NOT RUN).

- **Setting:** Settings "Dock style" list, stored `dockstyle=floating|attached`; default Floating (the current look, unchanged). Applies live: each dock moves, resizes and repaints; no new timer or surface.
- **Attached:** `layout::placement_styled` puts the dock flush on the work area's bottom edge (no 12 DIP margin), so it sits above a bottom taskbar and on the screen edge otherwise; per monitor, negative origins and tiny work areas covered by tests. Same width, height, slot grid and DPI maths, so slot hit-testing, drag-reorder, UIA bounds, dock menu and hover previews (all keyed to the window rect) need no change.
- **Look:** `graphics::attached_outline` is the notch outline turned over: flat bottom, top corners at `dock::RADIUS_DIPS` (`layout::attached_radius`, clamped to the body), filled with the theme's notch `body` and the same rim/hairline; Floating keeps `dock_fill` and its hover tint. Themes follow the palette. Not reused: `notch_outline` itself (its ears and bottom corners are the wrong way round; the attached path is 10 lines).
- **Hide/show:** unchanged (full-screen hiding, `--hide-dock`, Settings toggle).
- **Skipped:** the optional slide-up animation (would need a draw offset tween); concave ears at the bottom; a region clipping the transparent top corners from hit-testing (the floating bar has the same corners).
- **Tests:** placement of both styles (flush bottom, floating unchanged, notch ignores style), tiny work areas, radius clamp, key round trip, settings store round trip.
- **Risk:** the Settings window gains one row (taller); fit on small screens NOT RUN.
