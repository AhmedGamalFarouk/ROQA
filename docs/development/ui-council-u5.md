# U5 climb (council-5 round-2 defects; judges 7.2 / 7.8 / 7.1, target 8.0)

Native/GUI NOT RUN; the Chief's capture worker verifies by eye. Reviews paused.

## U5-A Timer tab as a control surface (defect 1)

- `timer::controls(view)` is the one list of Timer page buttons: idle or just finished = 5, 10, 15, 25 min and Focus (starts the Pomodoro); a plain timer = Cancel; a running Pomodoro phase = Pause, Skip, Cancel; a paused phase = Resume, Skip, Cancel. The controls act on the soonest timer, the one the page shows.
- The page title becomes the phase line ("Focus 2 of 4", "Short break", "Long break", plus ", paused"), from `Pomo::title`. A paused phase shows its remainder dimmed (`View::paused`); `Pomo` now lives in `Pages` so the paint can read it.
- Controls sit right of the digits (`layout::page_button(.., beside = true)`, 48 x 22 DIP at x 156, y 34); idle pills are 48 wide, 52 apart, so five clear the gear. Layout test covers both sets at all DPIs.
- Every control except a preset announces itself as a notice ("Focus 2 of 4: 25 min.", "Pomodoro paused.", "Timer cancelled."); the existing auto-advance notice stays.
