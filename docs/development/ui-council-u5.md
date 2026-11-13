# U5 climb (council-5 round-2 defects; judges 7.2 / 7.8 / 7.1, target 8.0)

Native/GUI NOT RUN; the Chief's capture worker verifies by eye. Reviews paused.

## U5-A Timer tab as a control surface (defect 1)

- `timer::controls(view)` is the one list of Timer page buttons: idle or just finished = 5, 10, 15, 25 min and Focus (starts the Pomodoro); a plain timer = Cancel; a running Pomodoro phase = Pause, Skip, Cancel; a paused phase = Resume, Skip, Cancel. The controls act on the soonest timer, the one the page shows.
- The page title becomes the phase line ("Focus 2 of 4", "Short break", "Long break", plus ", paused"), from `Pomo::title`. A paused phase shows its remainder dimmed (`View::paused`); `Pomo` now lives in `Pages` so the paint can read it.
- Controls sit right of the digits (`layout::page_button(.., beside = true)`, 48 x 22 DIP at x 156, y 34); idle pills are 48 wide, 52 apart, so five clear the gear. Layout test covers both sets at all DPIs.
- Every control except a preset announces itself as a notice ("Focus 2 of 4: 25 min.", "Pomodoro paused.", "Timer cancelled."); the existing auto-advance notice stays.

## U5-B Labelled tab rail and dock hover span (defects 2 and 6)

- The hanging handle is always open while the panel shows: one 44 x 22 DIP cell per tab with its name (10 DIP semibold), selected = accent wash, hovered = lighter ink wash (`Surface::tab_hover`, a repaint only when the cell under the pointer changes). 290 x 30 DIP for six tabs, so it stays narrower than the 320 plain panel and fits the 164 DIP window under the 120 DIP media or Limits panel. The dot state, the open/close tween, `tabs::Handle`, `refresh_rails` and the per-tab icons in the rail are gone (net deletion); the collapsed pill is untouched.
- Dock hover: the pop curve (0.3, 0.8, 0.5, 1.2) was front-loaded, 90 % done by 117 ms of 260, which is the ~110 ms the capture saw (the tween and the frame loop run the full 260 ms). New (0.45, 0, 0.3, 1.3): 90 % at ~143 ms, last visible change near 250 ms, 4.5 % overshoot. Test asserts the span.

## U5-C Glass backing and notch spacing (defects 3 and 5)

- Frosted and Liquid: `glass_scrim` fills a body-coloured backing (alpha 0.90, fading in with the panel) inset 9 DIP from the sides and bottom under the page content, so a busy backdrop cannot show through the text while the perimeter keeps the translucent glass and rim; the pill (no page) is unchanged. The tab rail fill is 0.95 in glass. No blur, no new surface.
- Plain panel 320 x 86 DIP (was 80): the notice row moves to y 70 and is 12 DIP (was 11; the media pocket keeps 11), Prayer countdown y 52 and the Hijri/credit footer y 70, so the Hijri line has a clear gap. `water::text` drops "(0 ml)" at zero glasses.
- Weather: plain hourly columns are 37 DIP apart from x 178 (last ends 289), clear of the gear at x 292.
- Limits: bars 24 DIP (was 14) with the percent and reset shifted to match, "as of" 10 DIP (was 9) at 55 %, Refresh pill y 11 and 15 high (was y 5, 17).

## U5-D Settings (defect 4)

- Buttons are as wide as their text plus 32 px (at least 120 DIP, measured with `DrawTextW` on the row's DC); Pomodoro rows read "Pomodoro: Start / pause" and "Skip to the next phase: Skip".
- Limits: a Copy button under the statusline field selects the whole command and sends `WM_COPY` (no clipboard API added); "Copied." in the footer. The field itself still scrolls (a native edit has no ellipsis).
- Drop-downs and edits may take 60 % of the row (was 50 %), so "Egyptian General Authority" fits at 240 DIP.
- `SIF_DISABLENOSCROLL`: the scroll bar stays (greyed) on short pages, so the content width and Close do not move between pages.
- The footer shows "Keep the dock or the notch on." only when that commit changed the dock or notch switch, and a page change clears the footer.
- Dark windows also set `DWMWA_CAPTION_COLOR` (sidebar colour) and `DWMWA_TEXT_COLOR` besides the dark-mode attribute that was already set; Windows 11 only, ignored elsewhere.
