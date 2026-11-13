# U7 climb (council-7 round-4 defects; judges 7.8 / 8.4 / 7.9, mean 8.03, goal met; margin pass)

Native/GUI NOT RUN; the Chief's capture worker verifies by eye. Reviews paused.

## U7-A One notice path, stale Pomodoro notice, glass rail, Light rail edge (defects 1, 2, 3, 5b)

- `layout::tall` grows the panel by `NOTICE_ROOM` while a notice shows on every page except the media pocket: the Limits page (any width) gets a 14 DIP row 6 under its content (`notice_slot`: y = page height - 16, 336 wide) and the panel eases 8 DIP taller. `notice_in_pocket` (media && no tall page) picks the pocket slot, font and alignment in `graphics::notice_text`. `NOTCH_WINDOW` 164 -> 192 DIP tall so the Limits page, its notice and the rail still fit (tests use the constant).
- Timer page actions clear a stale notice: a phase-start status (Resume, Skip, Focus) and the presets call `Host::clear_notice` (`Power::clear`, one repaint; the row eases out). Other statuses still replace it.
- Glass rail fill is fully opaque (1.0, was 0.97); the content backing keeps 0.97 and the unoccupied perimeter stays translucent. Light rail gets a 1 px hairline (ink at 0.22).
- Tests: `clear_drops_a_stale_notice_once`, `a_tall_page_gets_its_notice_row_under_its_content_inside_the_grown_panel`; window-size tests follow `NOTCH_WINDOW`.
