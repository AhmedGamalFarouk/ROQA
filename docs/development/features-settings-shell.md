# S1 sidebar Settings shell (2026-11-12)

Branch `feat/s1-settings`; landed f982f94 (W1 shell) and 9879fa6 (W2 controls + W3 settings). No native/GUI run (NOT RUN: drawing, switch hit and focus ring, dark mode, high contrast, RTL, DPI/150% text, keyboard walk, Narrator).

## Shape
- `settings_window.rs`: a `PAGES` table (page, cards, rows: id, kind, title, caption) drives one create loop, one layout loop and the `commit`/`show` id maps (`flag`, `pick`, `row_of`). Sidebar = owner-drawn `LISTBOX` (one tab stop, arrows); page switch hides the other pages' controls. Footer (status, Close) fixed; the page pane scrolls (`WM_VSCROLL`, wheel, focus into view). Paint is GDI with stock objects in `WM_ERASEBKGND` (cards, labels, title); no timer, thread or hook; the window exists only while open.
- Checkboxes stay native (`BS_AUTOCHECKBOX`) and are drawn as switches in `NM_CUSTOMDRAW`; colours follow the Windows app mode (registry read at open and on `WM_SETTINGCHANGE`), system colours in high contrast, system accent via DWM; a right-to-left UI sets `WS_EX_LAYOUTRTL` and `DT_RTLREADING`.
- **Not built** (size): stepper, segmented and tile controls (native drop-downs kept), monitor map, caption wrapping, About pinned to the bottom, Ctrl+Tab page jump (Up/Down on the list instead).

## Settings by page (store key; all old keys, defaults and live apply unchanged)
- **General:** `dock`, `notch`, `off=` per monitor, `desktop`; new `fshide` (Hide in full screen, default 1), `rsounds` (Reminder sounds, default 1).
- **Appearance:** `theme` (`glass` alias still loads), `dockstyle`; new `notchsize` (0 auto, 100, 115, 130, 150), `notchopen` (`hover`/`click`), `animspeed` (`normal`/`fast`).
- **Notch:** `tabmedia`..`tablimits`, `eyes`, `privacy`, `sound`, `shelfcap`, timer length (not stored) and Start.
- **Dock & edges:** `dbstart` and the other four dock-button keys, `edgevol`, `edgebri`, pins (combo, earlier, later; stored by the host).
- **Weather:** `detect`, `city`, `units`. **Prayer:** `premind`, `pmethod`, `plead`, `psound`; new `ppause` (default 0), `pshift=a,b,c,d,e` (Fajr, Dhuhr, Asr, Maghrib, Isha, -30..30).
- **Limits:** `limcodex`, `limagy`, `limgroup`, statusline text. **About:** `diag`, Reset to defaults, version.

## New settings: where they act
`fshide` `host.rs::refresh_fullscreen` (off: nothing remembered, regenerated at once); `notchopen` `set_notch_hover` + `click_control` (click mode: swell only, a click opens; UIA/keyboard always open); `notchsize` `layout::pill` (closed pill only, capped by the open panel; surfaces rebuilt like a theme change); `animspeed` `motion::paced` inside `Tween::retarget` (x0.6, spring frequency scaled, intent deadline too); `rsounds` gates the timer and prayer sounds in `complete_timer`/`fire_prayer` (`Settings::reminder_plays`/`timer_beeps`; not the Test button, which always plays and says so in its caption; the sound keys are untouched); `ppause` `fire_prayer` sends one `PLAY_PAUSE` only while a track plays, never resumes; `pshift` `prayer::day` adds minutes to the times that the page, countdown and reminders all use and rolls across midnight (`prayer::timeline` lists yesterday, today and tomorrow in today's seconds; neighbour days use today's UTC offset, so a DST-change night is off by an hour until after midnight); a typed shift outside -30..30 or not a number is reported in the status line and the box shows the kept value.

## UIA names (the control text or the label before it)
Sidebar list items: General, Appearance, Notch, Dock & edges, Weather, Prayer, Limits, About. Checkboxes (name = text, mnemonic kept): Show the dock, Show the notch, Show on monitor N (device), Show only windows on this desktop, Hide in full screen, Reminder sounds, Media/Weather/Prayer/Timer/Shelf/Limits tab, Mascot eyes, Privacy indicator, Play a sound when a timer finishes, Start/Search/Task View buttons, Minimized apps list, Show desktop strip, Volume/Brightness slider on the screen edge, Detect location automatically, Prayer reminders, Pause media at prayer time, Run codex/agy on Refresh. Drop-downs and edits (name = preceding label): Theme, Notch size, Open the notch, Animation speed, Dock style, Pinned apps, Temperature units, Prayer method, Heads-up before the prayer, Prayer sound, Antigravity limits to show; Timer length, Shelf size, Weather city, Fajr/Dhuhr/Asr/Maghrib/Isha shift, Statusline command, Diagnostics report file. Buttons: Start timer, Move pin earlier/later, Browse..., Test sound, Reset to defaults, Close. No "-"/"+" names exist (no steppers).

## Verification
Unit tests: table (every id on one row, every `Tab`/`Button`/toggle has a `flag`, every drop-down has rows), seven defaults and round trip, old store loads as today, clamps (`notchsize`, `pshift`, bad values ignored), `pill`, `paced`, prayer shift and next prayer. Store round trip and live apply in the host are unit-tested only up to `Settings`/`surface_diff`; window behaviour is NOT RUN natively.
