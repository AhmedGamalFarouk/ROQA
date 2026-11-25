# Moon on the dock, and the screenshot pill (design spec, 2026-11-23)

Designer spec for an implement worker. Build to it; do not redesign. Untracked.
Stills (PNG, 2x): `cook/design/moon-and-screenshot-pill/{classic,midnight,light,frosted,glass}.png` (all states per theme) and `states.png` (motion and queue, Classic). Mock source: HTML rendered by headless Chrome; the mocks use sample data (20 Nov 2026), not live ROQA.
Units are DIP. Tokens are the brand's (`cook/brand/BRAND.md` section 9): `body`, `tile`, `ink`, `accent`. "ink .55" means ink at 55% alpha.

---

## Part 1. The moon gets its own home

### Decision: a dock button that opens a popup on hover (not a notch tab)

| Question | Notch tab | Dock button + hover popup | Winner |
|---|---|---|---|
| Glance | Needs the notch opened, then a tab picked. The collapsed pill has no room (clock + eyes). | The icon itself shows today's phase, all day, with no action. | Dock |
| How often it is read | A few times a month. A seventh rail entry for a rare page adds clutter to the five-times-a-day Prayer and daily Media tabs. | One 44 slot in the right group, like Limits and Battery. | Dock |
| Space | 360 x 128 page, mostly empty for four facts. | 250 x 212 popup sized to the facts. | Dock |
| Brand | Fine either way. | The research rule "dock = where you go next" leans the other way, but the owner already put Limits (a status reading) on the dock with this exact pattern (commit 6013bfc); one pattern for glance readings beats two. | Dock, on consistency |
| Event-driven | Repaints with the notch. | Phase changes once a day: repaint the button when `refresh_clock` sees the date change (it already wakes once a minute for the clock). Popup is the existing hover popup; no new timer, thread or worker. | Tie |

Pick: **Moon on the dock.** The small moon glyph and phase name leave the Prayer page (the Hijri line stays where it is); the moon lives in one place.

### Placement
- Right button group, between the Limits gauge and the Quick sliders (order: tray chevron, battery, limits, **moon**, sliders). Button 44 x 44 (`BTN_DIPS`), icon drawn at 20.
- Switch: Settings > Prayer, "Moon on the dock", default on. Same mechanics as the Limits switch.
- Click does what hover does (opens the popup; a second click closes it). No tooltip (owner rule).

### Dock icon (collapsed state)
- 24 unit grid rendered at 20 (as all glyphs). Disc radius 9 units, centred.
- Ring: the full disc, stroke 1.75, ink .3 (Light: ink .55).
- Dark themes (Classic, Midnight, Frosted, Glass): the **lit share filled in ink**. Light: the disc filled in ink and the **lit share cut back to the dock body**, so a full moon is an open ring and a new moon a solid dot (never a black full moon).
- Why a fill when icons are outline-only: the lit share is a level reading, like the progress fill the brand allows. Outline-only was tried in the mock and a gibbous moon read as the letter O at 20 px.
- Shape: reuse `glyphs::moon` (limb arc, then terminator half-ellipse `|1 - 2 lit| * r` wide); it already draws all eight phases and mirrors south of the equator. Change only: ring alpha separate from the fill, and the Light inversion.
- Colour is ink, never accent.
- Hover: a 40 x 40 capsule behind the icon, ink .10, fades in with settle 260 (same as the other dock buttons' hover if one exists; reuse it).

### Popup (expanded state)
Uses the existing dock hover popup (`dock_menu::Menu::open_hover`, placed with `place`, anchored to the dock HWND as `anchored` rules require). Owner and dismissal unchanged.

```
panel        250 wide (edge LIST_W), radius 23 (row capsule 17 + inset 6, the one popover radius), pad 6 top/bottom
hero         92 high, side pad 14
  moon       64 x 64 at x 14, vertically centred
  name       x 92, 15/600 Display, ink           "Waxing gibbous"
  lit        12/400 Text, ink .7, tabular, +3    "84% lit"
  hijri      10.5/500 Text, ink .55, tabular, +3 "10 Jumada II 1448" (today's Umm al-Qura date, `prayer_os::hijri`)
divider      2 high capsule, inset 14 each side, ink .08, 4 below
rows         3 rows x 34 high, side pad 14; label 12/400 ink .86 left, date 12/400 ink .55 tabular right
  Full moon     Tue 24 Nov
  New moon      Wed 9 Dec
  1 Rajab       Thu 10 Dec      (first day of the next Hijri month, from Windows' calendar)
height       6 + 92 + 6 + 102 + 6 = 212 (+ the popup's 8 shadow margin each side)
```
- The three rows are labels (not clickable, no hover highlight), like `Item::Limit`.
- Hijri relation: the hero line says where in the Hijri month today is; the third row says when the next month starts. The new moon row sits right above it, so the link reads without words.
- Hero moon: disc filled `tile`; lit share filled token `moon_lit`. Light only: plus a ring stroke 1.75 ink .22 so a full moon still shows on `#ECEEF2`.

| theme | disc (dark side) | moon_lit | ring |
|---|---|---|---|
| Classic | `#3A3A40` | `#FFFFFF` | none |
| Midnight | `#283254` | `#E8EEFF` | none |
| Light | `#C6CAD1` | `#FFFFFF` | ink .22 |
| Frosted tint | rgba(255,255,255,.16) | `#FFFFFF` | none |
| Liquid glass | rgba(255,255,255,.16) | `#FFFFFF` | none |

### The numbers
- Phase name and lit share: `prayer_tab::moon` as is (mean synodic month, 8 names, within a day of the true phase).
- Percent: `round(lit * 100)`, shown as "N% lit".
- Next full / next new: the first local date after today whose noon age is nearest 0.5 / 0 (the same rounding that names a day "Full moon"), from the constants in `prayer_tab::moon`. May be a day off the almanac; say nothing about it in the UI. On the full or new day itself, the row shows the next one.
- Next Hijri month: step day by day from tomorrow (at most 30 calls of `prayer_os::hijri`) until the day is 1; show its month name and Gregorian date.
- When: computed once in `refresh_clock` when the date changes (and at start), kept on `pages` beside `hijri`. Hijri text is today only read while the Prayer tab is on; the moon needs it whenever the dock button is shown.
- Formats: dates "Tue 24 Nov" (short weekday, day, short month, user's locale order is fine), tabular numerals.

### Motion (moon)
- Hover capsule: settle 260 in and out.
- Popup: the existing hover-popup reveal, unchanged.
- Day change while the dock is visible: icon cross-fade, settle 260. Hidden dock: no frames, just the new state on next show.

---

## Part 2. The screenshot pill

### What it is
After a screenshot, the notch opens and its notice row shows a capsule chip with a thumbnail and two lines. Drag the chip to the Shelf to keep it, or into any app to drop it there. The image is held in memory only and never written to disk by ROQA.

### Layout (in the opened notch panel)
```
panel           360 x 128 + SHOT_ROOM 36 = 164 (replaces NOTICE_ROOM 8 for this row only)
chip            44 high, radius 22, fill `tile`, centred; top at 104 from panel top; 16 below it to the panel edge
  thumbnail     64 x 36 at inset 4, radius 18 (chip radius 22 - inset 4), image scaled to fill, centre-cropped
  gap           10
  line 1        12/500 Text, ink              "Screenshot" | "2 screenshots" | "3 screenshots"
  line 2        10.5/500 Text, ink .6         "Drag it anywhere" | "Drag the newest" | "Drop to keep or share" (dragging)
  right pad     18
```
Chip width follows the text (about 190). The page above (clock, eyes) is unchanged; eyes stay accent, nothing else in the chip is accent.

### States (see the stills)
| state | look |
|---|---|
| Rest | as above |
| Hover | chip fill + ink .07 overlay; thumbnail scale 1.04 (pop 360); cursor: hand. Hold deadline is held (absent) while hovered. |
| Dragging | thumbnail slot becomes an empty capsule (ink .12); line 2 "Drop to keep or share"; the panel page slides (slide 240) to Shelf, whose dashed drop zone (r14, the one allowed outline) reads "Drop here to keep it"; the drag ghost is the thumbnail at 128 x 72, radius 14, opacity .9 (Windows drag image). |
| Stacked (2-3) | the next-older thumbnail peeks behind, offset +8 x / -5 y, opacity .55; the top one gets a 2 px `body` ring to separate it; line 1 counts. |
| After a drop | the dragged shot leaves the stack. If none remain the row leaves (slide 240) and the notch closes as after any notice. |
| Shelf drop | the Shelf shows the image as a new item (thumbnail tile), kept in memory until removed or ROQA quits. |

### Timing
| step | curve | time |
|---|---|---|
| Notch opens | existing open spring | 500 bound |
| Notice row grows to SHOT_ROOM | slide | 240 |
| Thumbnail lands, scale .92 to 1, starts at +80 | pop | 360 |
| Text fades in, starts at +120 | settle | 260 |
| Hold | none (deadline) | 8 s (`PRAYER_NOTICE`), absent while hovered or dragging; leaving starts the full 8 s again |
| Row leaves | slide | 240, then the existing close spring |
Reduced/fast motion follows the brand rules (x0.6, no overshoot).

### Sharing the notice row (shared slot rule)
Shots live in their own in-memory stack, not in `Power`'s notice slot; the row shows the stack whenever the row is free. Nothing silently replaces anything:
1. **A plain notice arrives while the shot shows**: it is refused and waits in the one queue place (`Power::next`, as today); it shows when the shot leaves.
2. **A prayer, a due reminder (Snooze), an Eject or an agent question arrives**: it takes the row; the shot stack waits, then returns with a fresh 8 s hold. The shot is not lost and not shortened.
3. **A shot arrives while a notice shows**: the shot waits; it shows when the row frees (same `promote` moment).
4. **A second or third shot arrives while the pill shows**: it merges into the stack (count goes up, hold restarts). The stack holds 3; a fourth drops the oldest from the pill (Windows' own Snipping Tool and clipboard still have it). This is the only drop and it is the same writer's own stack.
5. While hovered or dragging the shot row is guarded like Ask/Snooze: nothing takes it.

### Memory
- Keep the clipboard image once (the DIB bytes) plus one small thumbnail bitmap per shot. At most 3 shots.
- Freed when the row retracts for good (no shots left to show), on a successful drop into an app, and on quit. Never written to disk, never logged, never added to clipboard history (which stays text only).

---

## Build list (in order; each step passes `verify.ps1`)

Moon:
1. `prayer_tab.rs`: extend `Moon` with `pct: u8`, `next_full: Ymd`, `next_new: Ymd`; add `next_month(ymd) -> Option<(String, Ymd)>` over `prayer_os::hijri`. Tests: known dates already in `the_moon_follows_known_new_and_full_moons` (next full after 2026-11-17 is 2026-11-24, noon age .494; next new after 2026-11-24 is 2026-12-09, age .002).
2. `host.rs` `refresh_clock`: when `clock.date` changes (and at start), compute the moon view once and store it on `pages`; repaint the dock button only then. No new timer.
3. `glyphs.rs` `moon`: separate ring alpha from fill; Light inversion (fill disc in ink, lit share in body). Keep the existing draw test.
4. `dock.rs`: a `Button::Moon` in the right group after Limits, shown when the switch is on; paints the 20 glyph at today's phase.
5. `dock_menu.rs` + `graphics.rs` `paint_list`: an `Item::Moon` hero row (92 high) and label rows with a right-aligned value; reuse `open_hover` and its placement. Click = hover.
6. Settings > Prayer: "Moon on the dock" switch (store key, default on), same path as the Limits switch.
7. `tab_art.rs` `prayer_page`: remove the small moon glyph and its name next to the Hijri line.
8. Live check on the second monitor only: icon at today's phase in all five themes; popup placement above the button.

Screenshot pill:
9. Detect a screenshot: on the existing clipboard update event, when an image format arrives and the clipboard owner's process is Windows' Snipping Tool / screen-clip host (verify the process names for Win+Shift+S, PrtScn and Win+PrtScn first; if PrtScn has no owner, accept any clipboard image while the owner decides, see OPEN). Read it once on the clipboard worker, not the UI thread.
10. A `Shots` stack (max 3, newest first) beside `Power`; the notice row shows it when `Power` has no notice; add the waiting rules above (`admits` treats a hovered/dragged shot row as guarded). Unit tests for the four queue cases.
11. Layout: `SHOT_ROOM 36` beside `ASK_ROOM`; draw the chip per the table; stacked and hover states.
12. Drag out: an OLE drag source (IDataObject with CF_DIB/PNG and a virtual file `Screenshot HH-MM.png` via CFSTR_FILEDESCRIPTOR/FILECONTENTS, so apps that only take files still work; the bytes are produced from memory on request). Drag image via the shell's drag-source helper. On drag start, select the Shelf page (slide 240).
13. Shelf: accept the in-memory image as a new item kind (thumbnail tile), kept until removed or quit; never saved by ROQA. Its own drag-out reuses step 12.
14. Live check on the second monitor: Win+Shift+S, then drag into Paint and into the Shelf; a Water notice during the pill; a reminder during the pill.

---

## Critic check (cook/RUBRIC.md, on the stills above)

| dimension | score | reason |
|---|---|---|
| Shape language | 8.7 | Capsules throughout; popup radius 23 = row 17 + inset 6; chip 22, thumbnail 18 = 22 - 4; the only outline is the Shelf drop zone. |
| Color | 8.8 | Accent only on eyes and the rail's selected bar; the moon is ink; Light uses `#9A5200` and inverts the icon fill correctly. |
| Motion / character | 8.2 | Three curves only, timings to scale in `states.png`; eyes keep watching while the pill shows. Judged from a timeline, not frames. |
| Icon set | 8.4 | Same grid, stroke and size as its neighbours; the filled lit share is the one deliberate exception (a level, like the battery). |
| Typography | 8.6 | Scale 15/12/10.5, tabular dates and percent, sentence case, all copy 5 words or fewer. |
| Coherence | 8.6 | Popup is the Limits popup's twin; the chip sits in the existing notice row with the existing rail. |
Blur at 8 px: yes. The notch is still a dark capsule with two warm eyes; the chip blurs to a lighter capsule under them.

Fixed during the check: outline-only dock moon read as "O" (now fills the lit share); Light theme full moon showed as a black disc (now inverted); popup clipped at the scene edge; the stacked thumbnail was invisible (offset raised); timeline labels overflowed their bars.

## OPEN (owner)
- Screenshot detection: only Windows' screenshot tools (recommended), or any image copied to the clipboard?
- Shelf keeps a dropped screenshot in memory until quit (recommended), or the Shelf refuses it and only app drops work.
- "Moon on the dock" default on (recommended) or off.
