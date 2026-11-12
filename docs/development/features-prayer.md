# P1: Prayer tab (2026-11-12)

> K1 (2026-11-13): keyboard and UIA/screen-reader support was removed by user decision. Keyboard, Tab, focus-ring, access-key and UIA lines below are history, not current behaviour.

Branch `feat/f2-prayer`. No native/GUI run, no real playback: all of it is NOT RUN visually or audibly.

- **Place:** the Weather feed's stored place (geocoded city or opt-in detection); no second lookup or prompt. The feed now also runs when only the Prayer tab is on. No city set: the page says "Set your city in Settings". Offline maths (`prayer.rs`), default Egyptian General Authority; Settings lists the four methods.
- **Page** (`prayer_tab.rs` view, `tab_art::prayer_page`): next prayer and "in 2 h 10 min" at the left, the five times as columns with the current one washed, Hijri date as the footer. Rebuilt from the minute clock's existing wake, only while the page shows. Hijri: `GetDateFormatEx` with `ar-SA` (Umm al-Qura is that locale's calendar; `GetCalendarInfoEx` does not convert dates), digits parsed, English month names; hidden if Windows does not answer in Hijri.
- **Reminder** (`Desktop::refresh_prayer`): one wait to the next event or a second after midnight; the day's events are recomputed when the date, UTC offset, place, method or heads-up changes, and on any time change or resume (`REPOSITION`). An event fires up to 120 s late; a resume later than that skips it; the first pass after start announces nothing. It reuses the power notice row (8 s, UIA live), opens the notches and plays the sound, unless the notches are hidden by a full-screen app. Master "Prayer reminders" checkbox (default on) and an optional heads-up (Off, 5, 10, 15 min).
- **Sound:** Settings list = None (silent) + audio files (`mp3 wav m4a wma`) in `assets/` next to the exe and up to three parents + a browsed file (path stored). Browse reuses the shelf's worker dialog (single file). Test plays it now. Default (empty setting) = first `takbeer*` file, else silent. MCI (`mpegvideo`, one alias) opens at play and closes at a host deadline (clip length + 0.5 s, max 10 min), a new play or exit. System volume only. No mp3 in the exe or git.
- **UIA:** the root name carries "Next prayer Asr in 2 hours 10 minutes"; Settings controls are standard named controls.
- **Idle:** no wake except the reminder deadline, the sound end, and the existing minute clock while the page shows.
- **Shortcut:** tomorrow's Fajr is not precomputed (the wait stops at midnight); a DST change at night is picked up by the offset in the schedule key.
- **Not done:** eyes mood at the reminder (the existing alert mood shows), per-prayer toggles.
- **Tests:** schedule (next deadline, midnight rollover, DST offset, stale after sleep, first pass), countdown text, Hijri parsing, sound discovery paths and default, settings round trip, tab order; OS: offset, Hijri 1448, missing-file play.
