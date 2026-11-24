# Notifications

Windows notifications and script `notify` lines get a clearer look and a calmer way to be handled. Other notices (power, drive, prayer, reminder, keys, progress, questions) keep their look.

## What is wrong today

- A Windows notification is the same flat 12 DIP line as a battery pill. Nothing says "this came from an app".
- It cannot be read slowly: 4 s whatever the length, and it still runs out while the pointer sits on it.
- It cannot be sent away: a plain notice has no click.
- A long line is cut at the row with no signal that it was cut.

## Interaction

- **Look:** a Windows notification or a `roqa.exe notify` line shows a small outline bell at the row's left (12 DIP, stroke 1.75, round caps, ink at 0.85, never accent), then the text. The text fills the rest of the row, end-ellipsis when cut. Same row, same entry rise and fade, no new motion, no new curve. Every other notice stays as it is (no bell).
- **Read time:** hold is 4 s up to 40 characters, 7 s above that. The cap is 140 characters already.
- **Hover holds:** while the pointer is on any plain notice row (not a question, Eject or Snooze row, which already wait), its end is held; leaving starts the normal 2 s delay, not the full hold. Same rule as anchored popups: a held deadline is absent, a release event starts the delay.
- **Click dismisses:** a click on a plain notice row closes it at once, after the 500 ms settle that questions use, so a click meant for something under it is not taken. A queued notice then shows as it does when a notice ends.
- **No** tooltip, no Narrator or UI Automation support, no sound, no badge, no new window, no new thread or timer.

## States

| State | Row |
| --- | --- |
| Windows notification, short | bell, text, 4 s |
| Windows notification, over 40 characters | bell, text with end-ellipsis, 7 s |
| Pointer on a plain notice | stays; ends 2 s after the pointer leaves |
| Click on a plain notice (after 500 ms) | closes, queued notice promotes |
| Click inside the first 500 ms | nothing |
| Row busy (question, Eject, Snooze, screenshot pill) | unchanged: one waiting slot, first refused stays |
| Notifications off in Settings | no hook, no bell, nothing new |

Copy: none new. The text is the app's own.

## Not in this round (owner may ask)

- A "missed" list of the last few notifications (a new surface, needs its own design; memory only, never saved to disk).
- Opening the app a notification came from on click (the banner gives no safe window to open).
- Mute per app, Do Not Disturb.

## Touch points

`src/toast.rs` (the line), `src/host/notices.rs` (`notification_peek`, hold by length), `src/power.rs` (a way to mark a notice as a bell notice, hover hold, click dismiss), `src/host/notch_clicks.rs` (the click), `src/graphics.rs` `notice_text` and `src/graphics/glyphs.rs` (bell glyph), `src/layout.rs` (text slot after the glyph). `host.rs` and `host/menus.rs` carry other threads' uncommitted work: touch only when clean.

## Tests

`power.rs`: hover holds then releases to a 2 s delay; click before and after settle; a bell notice still queues and still yields to a question. `toast.rs`/`notices.rs`: hold 4 s vs 7 s at 40 and 41 characters. `graphics.rs`: a notice scene with the bell and a long line draws without overlap.

No mock stills yet: the capture tool makes them with the build.
