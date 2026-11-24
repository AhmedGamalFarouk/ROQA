# Dock timer: paused chip and Resume

Owner decision 2026-11-24 (council r1, Sol finding 6). Pausing the only timer must not remove its dock chip, the dock must offer Resume, and a running chip's seconds tick on time. Everything reuses the existing chip, its hover popup (`dock_menu` rows) and the Timer page's pause/resume/cancel logic. No new surface, glyph, accent use, animation, setting or tooltip.

Stills (2x, `mock.html?view=...`): `dock-timer-resume/states.png` (chip states), `popup.png` (popup rows), `flow.png` (pause then resume), `themes.png` (five themes). The popup mock approximates the shipped `dock_menu` rows: the real rows keep their current metrics and drawing.

## States

| State | When | Chip reading | Chip ink | Popup rows (top to bottom) |
| --- | --- | --- | --- | --- |
| Running | a countdown runs (plain or Pomodoro phase) | soonest countdown as `m:ss`, `h:mm:ss` from one hour (`timer::clock(left)`), e.g. `24:59`, `1:02:03`. Soonest only, never `+N` | glyph and text at full ink (as today) | progress row `view.panel` (keeps `+N`, e.g. `4:10 +1`), **Pause**, **Add 5 min**: unchanged |
| Paused | a plain timer is held (`pages.held`) or a Pomodoro phase is paused (`pages.pomo.left`) and nothing else runs | frozen remainder, same format, e.g. `12:34` | glyph and text at **0.60** ink | progress row `Paused, 12:34 left` with the bar frozen at its progress, **Resume**, **Cancel** |
| Gone | nothing runs or is held; a timer just finished (the notch alert handles it); the stopwatch alone | no chip (as today) | n/a | popup closes (existing rule in `refresh_widget_readings`) |

- A running countdown always wins over a held one (`timer_view` order already does this): the chip shows the running one.
- The stopwatch never shows on the dock chip. Filter the view with `!view.watch && !view.done`.
- Copy: `Pause`, `Resume`, `Cancel` (the `timer::Ctl` labels), `Add 5 min`, `Paused, 12:34 left`. Same copy for a Pomodoro phase. Notices are the existing ones: "Timer paused.", "Timer resumed.", "Timer cancelled.", "Pomodoro paused.", "Pomodoro cancelled.". Leading with "Paused" avoids `12:34` reading as a clock time.

## Accent

None added. The Lamp stays only where it is today: the progress fill in the popup (frozen, still Lamp, when paused) and the existing 18% hover capsule behind the chip. The paused chip is never accent. Resume is a plain row like Pause, not an accent button: the two swap places, so they look alike.

## Interaction

1. The chip opens its popup exactly as today (hover, or click toggles it). Paused and running chips behave the same.
2. **Pause** (row 2): pauses via the existing `timer_press(Ctl::Pause)`. The popup stays open and its three rows update in place: **Resume** lands under the pointer where Pause was, **Cancel** where Add 5 min was. Same row count and kinds (`Limit`, `Widget`, `Widget`), so `update_rows` keeps the size and position: nothing jumps.
3. The chip stays in its slot, in paused ink. No relayout (it stays in `visible_dock_widgets`, so it still counts toward `widget_limit`).
4. **Resume** (row 2): `timer_press(Ctl::Resume)`; it already routes to `plain_act` (plain) or `pomo_act` (Pomodoro). Rows return to the running set in place; the chip goes back to full ink and ticks from the next second.
5. **Cancel** (row 3): `timer_press(Ctl::Cancel)`, which already clears a held plain timer or stops the Pomodoro. The chip leaves and the popup closes through the existing not-shown rule. Without Cancel a paused chip could sit in the dock forever; this is the only way out from the dock, so it is required.
6. Pausing or resuming from the notch Timer page updates the chip the same way.

## Hit targets

- Chip: the whole timer cell, 86 x dock cell height; its hover capsule is the cell minus 2 DIP each side, 82 x 36, radius 18 (existing). Unchanged.
- Popup rows: the existing `dock_menu` item rows, full popup width. Unchanged. The progress row is not clickable (as today).

## Drawing

- Reading: existing `draw_button` path, Segoe UI Variable 13 DIP, glyph 20 DIP at x 9, text from glyph + 5 to cell - 5 (47 DIP). Segoe digits are tabular, so ticks never shift the text. `1:02:03` fits; a countdown is capped well under 10 hours, so `h:mm:ss` is the widest.
- Paused ink: the Timer glyph `with_alpha(p.glyph, 0.60)` and the text `ink_dim(false, 0.60)`. Gives about n6 (`#999` on Classic, 7.4:1) and about 5:1 on Light. Pass one `bool` (timer chip paused) in `DockScene`; no other button changes.
- Five themes: surface and ink only, see `themes.png`.

## Motion

- Second ticks: no animation. The text swaps on the boundary, one dock repaint per second, only while the chip is on screen and running.
- Running to paused and back, chip ink: instant, no curve. Dock readings (weather, water, calendar) change instantly today, the change follows the user's own click, and the popup row text changes at the same moment.
- Popup rows: in-place update, no motion (existing `update_rows`). Popup open and close: unchanged.
- No pop, settle or slide is added. Nothing animates while paused.

## Timing (the tick bug)

- Today the chip shows whole minutes (`View::pill`, `24m`) and only refreshes on unrelated wakes. Switch the chip reading to `clock(view.left)` from `pages.timer_view` (filtered as above) in `refresh_widget_readings`; keep `pill` for the collapsed notch.
- Wake: the existing `timers.next_wake(expanded || widget_popup_is(Timer), now)` at `host.rs` (near the Timer refresh) also passes `true` when a running Timer chip is on screen in some dock (dock shown, not tucked or hidden by full screen). Make sure that wake runs `refresh_widget_readings`. Same `tick_in` boundary as the notch panel, so dock and notch change together. No new timer or thread.
- Paused: `next_wake` gets nothing from a held timer, so no wake at all. Dock hidden or tucked: no tick.
- `visible_dock_widgets`: `shown[5] &= <the filtered chip view is Some>` instead of `timers.running()`.
- `widget_rows(Button::Timer)`: read the same filtered view; running rows unchanged, paused rows as above. Add `TimerResume` and `TimerCancel` to `dock_menu::WidgetAction`, handled in `press_widget` with `timer_press(Ctl::Resume)` and `timer_press(Ctl::Cancel)`, then `refresh_widget_popup(Button::Timer)` as today.

## Tests (no live input)

- Pure: chip reading for 59 s, 25 min, 1 h 2 min 3 s; paused plain and paused Pomodoro keep the chip shown; stopwatch alone and finished timer hide it; running plus held shows the running one.
- Rows: running set is `Limit, Pause, Add 5 min`; paused set is `Limit, Resume, Cancel` with `Paused, m:ss left`; same discriminants (in-place update).
- Wake: `next_wake` returns the second boundary with the chip visible and the notch collapsed; nothing for a held timer.
- Live (second monitor, notch Timer page disabled): start a 2 min timer, watch the dock chip tick each second, pause from the chip popup, close it, chip stays dimmed, resume from the chip, cancel from the chip.
