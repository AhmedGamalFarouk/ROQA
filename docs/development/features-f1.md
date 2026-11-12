# F1: user feedback fixes (2026-11-12)

> K1 (2026-11-13): keyboard and UIA/screen-reader support was removed by user decision. Keyboard, Tab, focus-ring, access-key and UIA lines below are history, not current behaviour.

Branch `feat/f1-user-feedback`. No native/GUI run; every item below is NOT RUN visually.

1. **Eyes are occasional.** The pill shows the time. `eyes.rs` runs shows (blink, look around, smile) 40-100 s apart from one deadline (`Eyes::due`), plus a one-off for happy/surprised moods. A show is about a dozen stepped-opacity paints; between shows nothing paints. The time (plain pill) or title (media pill) fades while the eyes are up. Settings toggle unchanged.
2. **Volume UX.** Notch panel bar: 6 DIP track, thumb (4.5/6/7.5 DIP rest/hover/press), press-and-drag with mouse capture (`volume_fraction`, clamped), wheel over the bar (3 %, Shift 1 %), `NN%` readout while dragging, speaker glyph by level (`Glyph::speaker`). Edge pill: 10 DIP track, thumb that grows while pressed, same glyph and wheel steps. UIA slider semantics untouched. Deferred: eased value animation (needs a frame clock; values jump).
3. **Edge sliders not visible.** Cause: both are off by default (M5 gate 1, I1) and Settings could not be opened from the running app (item 4). The strip logic is unchanged (2 DIP hot strip, pill after 250 ms hover) and covered by the `edge.rs` tests; the Settings toggle reaches `edges_dirty` -> `reconcile` live. Not verified by hand: real hover, DirectComposition placement.
4. **Settings from the app.** Right-click on the notch, or on the dock outside an app slot, opens a menu (Settings, Quit ROQA) built on `dock_menu.rs`; a cog in the open panel's bottom-right opens Settings. The cog has no UIA element or keyboard stop yet; the menu does.
5. **File drag opens the Shelf.** A file entering the notch opens it and selects the Shelf tab (stored as the last tab); nothing changes with the Shelf tab off.
6. **Wake.** Start-up, resume (`REPOSITION` with resume), and coming back from the sleepy mood play a wake show (open, stretch, smile, fade). Lock and display-on are not hooked (would need `RegisterPowerSettingNotification` and session notifications); long-idle return covers them only when the pointer reaches the notch.

Tests: eyes schedule/wake/mood shows, wheel steps (Shift fine), drag clamp and gear placement, speaker glyph and thumb growth, app-menu placement, Shelf select.
