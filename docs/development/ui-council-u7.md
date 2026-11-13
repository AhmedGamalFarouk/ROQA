# U7 climb (council-7 round-4 defects; judges 7.8 / 8.4 / 7.9, mean 8.03, goal met; margin pass)

Native/GUI NOT RUN; the Chief's capture worker verifies by eye. Reviews paused.

## U7-A One notice path, stale Pomodoro notice, glass rail, Light rail edge (defects 1, 2, 3, 5b)

- `layout::tall` grows the panel by `NOTICE_ROOM` while a notice shows on every page except the media pocket: the Limits page (any width) gets a 14 DIP row 6 under its content (`notice_slot`: y = page height - 16, 336 wide) and the panel eases 8 DIP taller. `notice_in_pocket` (media && no tall page) picks the pocket slot, font and alignment in `graphics::notice_text`. `NOTCH_WINDOW` 164 -> 200 DIP tall (192 at U7-A, 200 with U7-C's taller rows) so the Limits page, its notice and the rail still fit (tests use the constant).
- Timer page actions clear a stale notice: a phase-start status (Resume, Skip, Focus) and the presets call `Host::clear_notice` (`Power::clear`, one repaint; the row eases out). Other statuses still replace it.
- Glass rail fill is fully opaque (1.0, was 0.97); the content backing keeps 0.97 and the unoccupied perimeter stays translucent. Light rail gets a 1 px hairline (ink at 0.22).
- Tests: `clear_drops_a_stale_notice_once`, `a_tall_page_gets_its_notice_row_under_its_content_inside_the_grown_panel`; window-size tests follow `NOTCH_WINDOW`.

## U7-B Settings polish (defect 4)

- About hint "Empty = off" (was "Diagnostics; empty is off", wrapped); Limits row "Antigravity models" (was "... limits to show", wrapped).
- A page that does not scroll (Weather, About) hides its scroll bar (`SIF_DISABLENOSCROLL` dropped); `layout` still reserves the bar's width (`SM_CXVSCROLL`) so content and Close do not move between pages, it is just an empty gutter.
- Skipped: statusline field end-ellipsis (a native Edit cannot ellipsise; it would need owner drawing, not cheap). Copy still carries the whole command.

## U7-C Limits rows (defect 5)

- Two lines per tool (`layout::LIMITS_ROW` 22 -> 30; `limits_height(3)` 126 -> 150 incl. the notice row's room): line 1 the name and, per window, a 44 DIP bar with its aligned used value (`51%`; `reset` for an expired window); line 2 the labelled reset ("resets 13:39", "resets Sun 08:59"; "at 13:39" after a reset) and, under the name, the freshness ("as of 16:56" / age). Columns 88 and 204, headers "5h used" / "Weekly used" over them; no right-hand "as of" column.
- Adaptive height follows the tool count (`limits_height`), window 200 DIP so the grown panel plus overshoot and rail fit (tested).
