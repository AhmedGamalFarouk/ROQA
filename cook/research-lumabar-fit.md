# LumaBar ideas for ROQA

Source review, November 23, 2026, against ROQA `e70dc9f`. Existing browser, Limits and taskbar/tray edits are out of scope. Neither app was live-tested.

## Releases, not promises

- Sources: [widget catalog](https://lumabar.xyz/widgets), [docs](https://lumabar.xyz/docs), [changelog](https://lumabar.xyz/changelog), and [GitHub releases and README](https://github.com/Acadbek/lumabar). The repo supplies releases, not app source. The catalog, not the older README count, defines the inventory. Sample account data is not evidence of a working connection; **Cursor Usage** is explicitly Preview. Nothing from roadmap counts as shipped.
- **1.2.29 (November 23):** latest GitHub release at review time. Adds multilingual typed actions in Search.
- **1.2.28:** inline tray selection, optional ungrouped windows, device batteries, clean style, up to six world clocks (catalog still describes three), city selection, and restoring the taskbar from Settings. The site labels it November 23; GitHub records November 22 at 19:45 UTC.
- **1.2.27 (November 21):** file location, USB eject in Quick Toggles, middle-click relaunch, magnification speed, calendar sync button and fullscreen popup deferral. Changelog lists Recently Opened in 0.1.6 (September 28).

## Candidates, ranked

1. **Open file location.** One row after New instance for top-level app tiles with a resolved local executable identity. Explorer selects the file. No new settings, scanning, persistent worker or persistence. Omit generic app IDs and nonlocal or ambiguous paths; revalidate the clicked target without blocking the UI. See [design](design/open-file-location.md).
2. **USB eject in Places.** ROQA already ejects from its arrival alert. Add an on-demand route after dismissal, reusing Places data and the eject worker. A reused drive letter must never eject a different device; defer if safe identity cannot be established.
3. **World Time.** One optional second zone in expanded Clock, as in the wish list. Reuse the clock update; account for daylight saving and different dates. No extra dock tile or six-city grid.
4. **Recently Opened.** ROQA keeps three recently closed apps in memory, not recent files. Consider a bounded list in Places from Windows' existing recent items, read only on demand. No history collection, new persistence or remote paths.
5. **Commands / Search.** Native Windows Search and quick links already exist. There is no general typed-action surface; the wish list's old picker reference points to dialog helpers. Add only if a few named commands justify it. No arbitrary execution, indexing or multilingual parser.

Borrow short summaries and reveal detail on demand. Keep notch and dock separate, with existing capsule styling, Lamp and the existing motion clock.

## Already in ROQA or covered by a native route

| LumaBar feature | ROQA equivalent and limit |
| --- | --- |
| Apps, Start, pins, running windows, groups, previews, middle-click, close and minimize | Dock and owned menus already cover these. Jump Lists and ungrouped windows are not parity. |
| Taskbar hide/restore, launch at startup, fullscreen awareness | Existing behavior; no claim that every alert is replayed identically. |
| AI Limits, Claude Usage, Claude Plan Usage, ChatGPT Usage | Limits already covers Claude and Codex quota/reset, plus Antigravity. No claim of per-model or general Gemini account parity. |
| Pomodoro, Timer, Stopwatch | Existing controls; no need to split them across the dock. |
| Calendar, 7 Days, Today, Date, Day Number, Clock | Local calendar and clock. No account agenda or always-on seconds required. |
| Battery, Device Batteries | Battery pill and on-demand Windows-reported device readings; hardware coverage depends on drivers. |
| Tray, Notifications, Show Desktop, Brightness | Existing entry points and brightness control; inline tray selection is a separate feature. |
| Now Playing, Audio Output, Microphone | Existing playback/seek, volume/output and mic mute; no need to bundle speaker mute with it. |
| Recycle Bin, drive browsing, Screenshot Capture | Places offers bin and drives; screenshot tools already exist. |
| Weather, Weather Details, Today & Forecast, Weather Status, Hourly | Existing conditions and forecast; presentation and range differ. |
| Sunrise & Sunset | Related prayer times, no separate sun-position dial. |
| Appearance, taskbar and dock controls | ROQA's five themes and monitor/visibility settings; no reason to pursue layout parity. |

## Defer

| LumaBar feature | Why not now |
| --- | --- |
| Google/Outlook/iCloud sync; payouts and renewals on Calendar | Auth, recurring events, privacy and refresh turn a local calendar into a separate product. |
| Todos, Notes | Reminders and Shelf overlap, but are not durable task/notes apps. No new storage without demand. |
| WhatsApp, Gmail, GitHub, GitHub Overview, GitHub Activity, LeetCode Stats | Windows notifications and pinned apps cover basic access. No mail or activity mirrors in this slice. |
| Gemini Usage, Grok Usage, DeepSeek Balance, GLM Usage, OpenCode Usage, OpenCode Plan Usage, Cursor Usage (preview) | Separate provider decisions. Require verified sources and opt-in; no signed-in browser scraping. |
| Currency, Stock Chart, Stock Watchlist, Crypto | Outside the dock's job; price feeds add freshness and error handling. |
| Total Revenue, Polar, Stripe, Dodo Payments, Shopify, Lemon Squeezy, Paddle, Gumroad and their chart variants | Account-specific dashboards, credentials and updates. |
| Search Console, DataFast Chart, Plausible, Dub, Resend, Linear, Supabase | Same concern for analytics and developer accounts. |
| X Profile, TikTok, YouTube, Instagram, Wallpaper Gallery | No strong dock need. |
| Quick Toggles, Theme Toggle, Do Not Disturb, Keyboard, Language | Use native Windows entry points. Don't bring back global key monitoring for language. |
| Storage, Top CPU, Top RAM, Temperature, CPU, RAM, Network | Use Explorer or Task Manager. Consider only user-requested on-demand summaries, not always-on sampling. |
| Inline tray, ungrouped windows, Jump Lists, dock profiles, top/bottom widget-dock, updater UI | Wider behavior, scope or distribution questions. Don't overlap pending tray work. |

## Reject for this direction

| LumaBar feature | Why it doesn't fit |
| --- | --- |
| Screen Time and Day Timeline with per-app/site history | No persistent activity surveillance. Existing focus timers are explicit and user-controlled. |
| Flip Clock, Analog Minimal, Segment Clock, multi-city World Clocks grid, Clock & Weather blocks | Busier duplicates. Borrow one optional secondary zone, not the widget wall. |
| Battery Ring/Core/Dots/Gauge, Month/Day/Year Progress, Now Playing compact/Mini/Bar copies | Don't add multiple presentations to fill space. |
| Custom radius/accent, magnification-speed tuning, copied glass, widget picker and free-form widgets | Keep ROQA's existing shape, material palettes and event-driven motion. |

## Local evidence and implementation gate

- Read `README.md:22`, `graft/src/dock_menu.md:1`, `graft/src/host/menus.md:1`, `graft/src/places.md:1`, `graft/src/widgets.md:1`, `graft/src/pins.md:1` and `FEATURES_TO_ADD.md:68`. Treat wish-list entries as hypotheses, not proof of current behavior.
- `src/dock_menu.rs:163` lists app actions and `src/dock_menu.rs:202` checks menu validity; `src/pins.rs:358` includes shell launch targets. `src/dock.rs:77` has process metadata separately from app identity. Do not assume a packaged or browser host executable represents the app.
- `src/host/menus.rs:246` opens Places on demand; `src/places.rs:251` enumerates drives; `src/host/notch_clicks.rs:76` calls the existing eject worker. `src/dock.rs:88` bounds recent apps; `src/host/menus.rs:687` opens native Windows Search.
- No code or tests until `/ponytail` is available or Ahmed approves the manual smallest-change rule. Then use Sol 6.2, run `scripts/verify.ps1`, record the release byte delta and check Explorer behavior on the second monitor without interrupting Ahmed's ROQA session.
