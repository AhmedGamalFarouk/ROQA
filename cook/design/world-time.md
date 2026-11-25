# World time

One optional second Windows time zone, one text line in the open Clock page. Off by default. Borrow only LumaBar's single secondary zone, not its city grid.

## Interaction

- **Pick or remove:** Settings, Notch page, new card **Clock**, one drop-down row **Second time zone** (hint "Under the date, open clock"). First row **Off** (the default), then every Windows zone, west to east, text `(UTC+09:00) Osaka, Sapporo, Tokyo`. Choosing a zone turns it on; choosing **Off** removes it. No add button, no second control, no per-zone options. Same combo, card, hint and capsule styling as the Weather and Notch cards; the chosen row is the accent capsule, as in every combo.
- **Notch:** the open Clock page (the Media page without a track, the page every tab starts from) gains one centred text line under the date: `Tokyo 3:41 AM, tomorrow`. Nothing else changes: collapsed pill, playing-media view, other pages, dock, menus and Settings tiles show no zone. No dock tile, no click target, no hover, no tooltip, no new motion; it fades in with the date through the existing `reveal`.
- **Style:** Segoe UI Variable Text 11 DIP, ink at 0.60 (the date's strength, one step above the Hijri line's 0.45), centred in the same left zone (width 266 DIP at 420). No accent, icon or divider.
- **Row slot:** the first free row under the date. Hijri off: the Hijri slot (centre 100). Prayer tab on: one row lower (centre 118, bottom of the 140 DIP panel, clear of the 40 DIP corner at this width). Never overlaps the mascot zone.
- **Text:** `<label> <time>` plus `, today` is never shown. Same Windows day: nothing. Other day: `, tomorrow` or `, yesterday`; a gap of two days (UTC-12 against UTC+14) shows the zone's short weekday instead (`, Fri`). Time uses the Windows locale and its 12/24 hour choice (`GetTimeFormatEx`, no seconds), exactly as the main clock. 7 words or fewer, no em dashes.
- **Label:** the Windows display name minus its `(UTC...) ` prefix; if it has commas keep the last item; drop a trailing `(...)`; cap at 22 characters with an ellipsis. `Osaka, Sapporo, Tokyo` becomes `Tokyo`, `Eastern Time (US & Canada)` becomes `Eastern Time`, `Dublin, Edinburgh, Lisbon, London` becomes `London`.
- **Defaults unchanged:** empty value means off; no new window, tick, thread, timer, network call or dock work. Reset to defaults clears it.

## States

| State | Notch | Settings field |
| --- | --- | --- |
| Off | exactly today's page | Off |
| On, same day | `London 6:41 PM` | chosen zone |
| On, other day | `Tokyo 3:41 AM, tomorrow` / `, yesterday` | chosen zone |
| Prayer tab on | zone line below the Hijri line | unchanged |
| Stored zone unknown on this PC (renamed by an update, hand-edited store) | no line, silently | extra row **Not on this PC**, selected. Nothing is deleted until the owner picks another row; saving other settings keeps the stored key |
| DST change in either zone | text follows the next minute tick | no change |
| Windows clock, zone or locale change, resume | line follows with the main clock (see below) | no change |

No error copy in the notch: the owner cannot act there, and the field in Settings says why.

## Data and timing (no new work)

- **Store:** one line `world=<TimeZoneKeyName>` (for example `Tokyo Standard Time`), written only when non-empty, like `city`. `Settings::set` accepts a non-empty value up to 64 characters with no control characters; anything else is rejected and the old value kept. Default `""`, already clamped.
- **One tick:** `clock::now` already wakes once per minute (`next_minute_ms`) and `refresh_clock` repaints only when text changed. Every Windows zone is whole minutes from UTC, so the zone line changes on the same boundary. Compute it inside `clock::now(hijri, world_key)` from the same instant; add `world: String` to `Clock`. No second deadline.
- **Conversion:** one `GetSystemTime` then `SystemTimeToTzSpecificLocalTimeEx(&dynamic_zone, &utc)`, which applies that zone's own DST rules for the year. Day offset from the zone date against `local`'s date.
- **Resolve once:** map the key to a `DYNAMIC_TIME_ZONE_INFORMATION` by walking `EnumDynamicTimeZoneInformation` until `TimeZoneKeyName` matches. Cache the result keyed by the key string in a `static` in `clock.rs` (a `Mutex<Option<(String, Option<Zone>)>>`), so the walk runs only when the key changes, not every minute. No match caches `None`. Settings fills its list from the same walk, display text from `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Time Zones\<key>` `MUI_Display` (`RegLoadMUIStringW`), falling back to `Display`, then the key. Sort by `Bias` descending, then display text. The `Win32_System_Registry` feature is already enabled; confirm `Win32_System_Time` for the two time calls.
- **Refresh on change:** the existing `clock_due = Instant::now()` on time, zone and locale broadcasts (`host.rs` near the topology request) already covers a system clock or zone change. Add the same reset in `apply_settings` when `world` changed, so the line appears at once.
- **Repaint test:** `refresh_clock` compares `clock.world` with `pages.world` beside time and date. Hidden notches stay at zero work.

## Settings details

- `settings_window.rs`: new `Card { label: "Clock", rows: [r(WORLD, Kind::Combo(300), "Second time zone", "Under the date, open clock")] }` after "In the notch". `fill_combo(WORLD)` adds **Off**, then each zone; `row_of`/`pick` follow the `UNITS` pattern, with the **Not on this PC** row added only when the stored key does not resolve, and picking it leaves the value unchanged. `draw_combo` already end-ellipsizes; the longest names fit 300 DIP at 12 DIP text, verify at 150% DPI.
- Settings opens on the UI thread today; the zone walk is a few registry reads, measured at implementation. Over 2 ms, do it on the existing Settings worker path, never a new thread.

## Integration boundary (pending edits)

`host.rs`, `host/menus.rs`, `system_os.rs`, `places.rs`, `dock_menu.rs`, `anchored*`, `window_catalog.rs`, `limits.rs`, `shelf_os.rs` and `taskbar.rs` hold other threads' uncommitted work, including the in-flight USB eject and Open file location. `git commit -- <path>` takes a whole file, so do not touch those until each is clean (`git diff --quiet <path>`).

Order, each step compiles and passes `verify.ps1` on its own:
1. `clock.rs` (zone type, resolve cache, label and day-text pure functions, `world` field, tests). Untouched by others.
2. `settings.rs` (field, key, write rule, tests). Untouched.
3. `settings_window.rs` (card, combo, missing-zone row). Untouched.
4. `graphics.rs` and `graphics/tab_art.rs`: `NotchScene.world: &str` and the draw call after the Hijri block in `media_page`; the five test scenes get `world: ""`. Untouched, but step 5 must follow at once or the build breaks, so land 4 and 5 together.
5. `host.rs` (4 small hunks: `Pages.world` field and init near `hijri`, the `clock::now` call and compare in `refresh_clock`, the scene field near line 877) and `host/settings_bridge.rs` (`clock_due` reset). Do this only when `host.rs` is clean. If it is still dirty, stop after step 3 (a settable, stored, silent zone) and report; do not partial-commit someone else's hunks.

Ceilings: `host.rs` stays under its no-growth ceiling (about 10 lines net); say so in the report if it moves.

## Focused tests (no live windows)

- `clock.rs`: label shortening cases (above, plus `UTC`, no-comma, cap); day text (same, +1, -1, two-day weekday) as a pure function over two dates; unknown key resolves to none and yields an empty line; fixed UTC instants against real Windows zones: Tokyo never shifts, Eastern at 2026-03-08 06:59 and 07:00 UTC (spring forward) and 2026-11-01 05:59 and 06:00 UTC (fall back); rollover at a fixed instant.
- `settings.rs`: round trip, empty not written, over-long and control-character values rejected, reset clears, `every_field_changed` extended.
- `graphics.rs`: with and without `world`, row slot moves below Hijri, nothing drawn on the collapsed pill or the track view.
- Settings combo: Off first, sorted west to east, missing key adds the row and keeps the value.
- Live check, second monitor only: open the notch, pick a zone, line appears; change Windows time zone and time format; Prayer tab on; five themes at 100% and 150% DPI. `scripts\verify.ps1`, report the exe byte delta (expected small, no new dependency).

## Stills and limits

Stills are static HTML in headless Chrome, not Direct2D captures, with sample data (24 Nov 2026, Cairo local): [states.png](world-time/states.png) (off, same day, tomorrow, yesterday, with Hijri, 24-hour), [themes.png](world-time/themes.png) (five themes), [settings.png](world-time/settings.png) (Classic and Light: off, open list, chosen, missing). Panel geometry follows `media_page`'s rows; exact kerning, the rail below the panel and the notch top flare are not drawn.

Not verified: `EnumDynamicTimeZoneInformation` and `MUI_Display` on this machine; localised display names on non-English Windows; labels for rare zones longer than the cap; cost of the zone walk. Hidden-taskbar and multi-monitor behaviour is unaffected. No independent critique round (no subagents in this thread). Rejected: city grid, dock tile, per-zone label editing, seconds, flags or maps, network lookups, a second tick.
