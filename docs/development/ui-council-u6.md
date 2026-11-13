# U6 climb (council-6 round-3 defects; judges 7.6 / 8.2 / 7.8, mean 7.87, target 8.0)

Native/GUI NOT RUN; the Chief's capture worker verifies by eye. Reviews paused.

## U6-A Glass isolation (defect 1)

- The Frosted/Liquid content backing now follows the body's own outline inset only `layout::BACKING_INSET` (2 DIP, bottom corner radius 24 - 2) instead of 9 DIP, at alpha 0.97; the tab rail uses the same 0.97 (was 0.90), so backdrop text no longer shows through either. Only an unoccupied 2 DIP border stays glass.
- Plain panel 86 -> 92 DIP. Footers (Weather credit, Hijri date) come from `layout::footer` (a step larger, 11 DIP, bottom 12 DIP above the body's bottom); the gear moved to 16 from the right / 10 from the bottom. Test `every_footer_and_the_gear_sit_six_dips_inside_the_glass_backing` measures each footer corner and the gear's disc against the rounded backing (>= 6 DIP) for both panels.

## U6-B Notice row (defect 2)

- The plain panel grows by `layout::NOTICE_ROOM` (8 DIP, 92 -> 100) while a notice shows, eased by the notice's own entry tween (`notice_fx`, the motion clock) and eased back as it leaves; `layout::set_notice_shown` carries it to the shape like the page height. The notice row moved to y 76: 6 DIP under the Timer pills, 10 above the bottom. The media panel keeps its pocket notice and the Limits page (no spare window height) does not grow.
- Phase starts and resumes ("Focus: 25 min.", "Short break: 5 min.") no longer raise a notice from the Timer page buttons: the title line already says it (`pomo::is_start_status`; errors, pause and cancel still show). Tests: `a_notice_grows_the_plain_panel_by_its_row_and_leaves_padding`, `only_a_phase_start_line_is_redundant`.

## U6-C Settings feedback and wording (defect 3, polish)

- A refused switch-off of the last of dock/notch now says "Keep the dock or the notch on." The old test compared the clamped result with the current settings, which snap back to equal when the notch is the one left on, so it never fired; `footer_status` decides from the REQUESTED values (`kept_a_kind` is computed before clamping). Test `a_refused_last_switch_says_so_instead_of_applied`.
- Weather > Units drop-down items no longer repeat the row label ("Automatic", "Celsius (°C)", "Fahrenheit (°F)"); About > "Diagnostics report file" is now "Report file" under the Diagnostics card (no wrap), hint "Diagnostics; empty is off".

## U6-D Light accent (defect 4)

- Light now takes the system accent where it reads, deepened a fifth (`theme::deepen`; blue 0,120,215 becomes 0,96,172, contrast >= 3 against the body checked like the other themes), so the selected tab and the next-prayer chip are an accent wash with dark accent text, like Classic and Midnight; a pale accent still falls back to ink. Test `the_accent_colours_fills_and_rings_only_where_it_reads` updated.

## U6-E Dock hover (defect 5)

- Cause, from the numbers rather than the curve alone: the lift (5 DIP) and growth (10 %) were rounded to whole pixels every frame, so they showed only as ~10 one-pixel steps, and with the steep middle of the old POP curve (10 % to 90 % in 94 ms of 260) all of them fell inside ~100 ms; the first frame also timed its tween from the mouse message, so a late first paint (icon upload) started already inside the curve.
- Fix: `graphics::icon_metrics` moves size and lift smoothly while the hover animates and rounds only at rest (hover 0 or 1, keeps bitmaps crisp); `motion::POP` is now Bezier(0.3, 0.1, 0.55, 1.15) (5 % at ~22 ms, 95 % at ~190 ms, <= ~10 % of the travel per 16 ms frame, ~1.5 % overshoot); `Surface::paint_dock` restarts the running hover tweens' clocks at the first frame after a hover change (`Tween::restart`, settled tweens untouched), so that frame shows the rest value. Tests: `the_dock_pop_moves_visibly_across_most_of_its_duration`, `a_restarted_tween_shows_its_rest_value_on_the_first_frame`, `the_hovered_icon_moves_smoothly_and_rests_on_whole_pixels`.

## U6-F Limits secondary text (defect 6)

- Limits window labels, reset times and "as of" go from 10 / 10.5 to 11 DIP (`SECONDARY`), the row pitch from 20 to 22 (`layout::LIMITS_ROW`, so `limits_height` adapts: 126 DIP for three rows), and the columns were re-spaced for the wider text (bar 20, percent and "reset" ending at x+50, reset time from x+52, 36 and 56 wide; "as of" 66 wide from x 286). Hijri date and Weather credit went to 11 DIP in U6-A; Settings wording in U6-C. Test `the_limits_page_grows_with_its_rows_and_still_fits_the_window_with_the_rail` (three rows, open overshoot and rail inside the 164 DIP window).
