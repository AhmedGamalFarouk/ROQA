# ROQA product review (2026-11-18)

Role: critic, read-only. Commit `11a7c21`, exe 1,487,360 B. ROQA was used live on display 2 (its notch and dock live there; the owner runs Nusa and Nectar on display 1).
Screenshots: `artifacts/product-review/` (local and gitignored, never committed). Names are given per item.
Competitors were checked from their public pages and reviews: NotchNook (lo.cafe), Alcove, Boring Notch, DockDoor, uBar, ObjectDock 3, with Nectar and Nusa running on this PC.
Items already built, ruled out or owner-decides in `FEATURES_TO_ADD.md` are left out. Rule column: does fixing it, or the problem itself, touch an AGENTS.md rule?

## Measured
- Footprint with a song playing and every page closed: 67 to 71 MB working set, 34 to 38 MB private, 23 to 26 threads, about 650 to 720 handles.
- CPU with a song playing and nothing open: 0.4 s, 0.9 s and 1.6 s per minute (about 1 to 2.6 % of one core). The live level bars in the closed notch read the audio peak and redraw for as long as music plays.
- Crashes: 16 in the Windows log on 15 and 16 Nov (user32, access violation), none since 16 Nov 00:50.
- Nectar for comparison: 22 MB working set in its own process, plus a share of 12 WebView2 processes (491 MB in total, shared with other apps).

## Do better (ranked)

| # | What is wrong | Why it matters | Size | Rule |
|---|---|---|---|---|
| 1 | The prayer sound is set to `takbeer2.mp3` in an old folder (`.t3\projects\roqa\assets`) that no longer exists. Settings still shows the name, and at prayer time ROQA plays nothing and says nothing. | The adhan will be silent on the owner's own PC. Fall back to the built-in sound and show "Sound file missing" on the row. (`s5-prayer-scrolled.png`) | S | none |
| 2 | The open notch also draws the closed row behind it: the track time and "12:18 pm" show faintly through the song title. The hover state does the same, with the title showing under the time. | It is visible every time the notch opens and reads as a rendering bug. (`n00-ghost-zoom.png`, `n00-open-settled.png`, `n01-hover-swell.png`) | S | brand (coherence) |
| 3 | With auto-hide on, a window covering the dock's spot on display 2 kept it hidden. Neither resting at the bottom edge nor `--show-dock` brought it back in my test. | If the edge reveal is broken, the dock is gone until the window moves. Synthetic pointer moves were used, so the owner should try it by hand. (`d0-dock-reveal.png`, `d1-dock-hover.png`) | M | none |
| 4 | Tabs are hard to find. The open panel showed only Media, with no visible tab rail. Moving just below the panel closed it, and the wheel did nothing on the hovered (not yet open) pill. | Weather, Prayer, Timer, Shelf, Limits and Widgets are invisible to someone who doesn't already know about them. Competitors show tab icons along the top of the open panel. (`n10-rail-before.png`, `n11-rail-hover.png`) | M | none |
| 5 | The closed notch's level bars keep redrawing for as long as music plays, at 1 to 2.6 % of a core. | Music plays for hours, so this is ROQA's real idle cost. Offer still bars, or draw them only while the notch is open or hovered. | S | rule 1 (frames only while animating) |
| 6 | The capture tool can't be used on this PC. On two monitors its notch stills came out blank gray, and the run stopped with "Another ROQA is still running" because ROQA was still exiting after the tool's wait. | Design rounds can't be scored, and slow quitting is itself a reliability smell. The new owner rule (tests on the second monitor) needs the tool to aim there. | M | tooling only |
| 7 | Settings opens on display 1 even though ROQA's notch and dock live on display 2. | You look for it on the wrong screen, and it breaks the new "test on the second monitor" rule. Open it on the monitor whose notch was clicked. | S | new owner rule |
| 8 | Twice on display 1, Settings drew pages with missing labels, empty boxes and a stray "Empty" box over Close: once on the first switch to Notch, once after a wheel scroll on Prayer. It did not happen again on display 2. | It looked broken when it happened. Unconfirmed: needs a check by hand. (Screenshots deleted under the second-monitor rule.) | S | brand (coherence) |
| 9 | "ROQA on" on the Displays cards is in Lamp amber. | BRAND.md lets the accent appear only on eyes, the primary action, progress, the selected indicator, the today marker and the prayer sun. Plain status text should use ink. (`s0-general.png`) | S | brand |
| 10 | Caption indents differ. Captions under toggles are pushed right ("Open ROQA when you sign in", "Tint progress bars…"), while captions under drop-downs line up with their label. | It looks unfinished next to Windows 11 Settings. (`s0-general.png`, `s1-appearance.png`) | S | brand |
| 11 | Crash reports are off by default: Diagnostics "Report file" is empty. | The 16 crashes on 15 and 16 Nov left only the Windows event log. A default log in `%LOCALAPPDATA%\ROQA` would catch the next one. (`s8-about.png`) | S | none |
| 12 | The size figures are stale. AGENTS.md says "~1.18 MB exe" and the README says "~1 MB"; the exe is 1,487,360 B (+26 % since r12). | The "tiny" claim is part of the product story, so keep it true. There is no size limit (owner decision), so this is a docs fix only. | S | none (docs) |
| 13 | About shows only "ROQA 0.1.0". | There is no build date or commit, so it is hard to tell which build is running when reporting a bug. | S | none |

## Lacks (ranked)

| # | What is missing | Why it matters | Size | Rule |
|---|---|---|---|---|
| 14 | ROQA is not the owner's main-screen app: Nusa and Nectar run on display 1, and Start with Windows is off. | This is the plainest product signal. Ask what Nusa and Nectar still do better, then close those gaps before adding more. | owner | none |
| 15 | No first-run moment: with a fresh store nothing explains hover, the tabs, Settings or the shelf (already listed as "First-run greeting" in the left-over list, still not built). | Every competitor has a short welcome. Points 4 and 14 make this the cheapest discoverability win. | S | none |
| 16 | No search in Settings. There are 9 pages with about 80 rows (hotkeys, edges, prayer fine-tune and more). | It is hard to find "Hide behind windows" or "Heads-up before the prayer". A filter box over the existing rows is enough. | M | none |
| 17 | No live-activity pill for Bluetooth headphones connecting or for browser downloads. | These are the most-praised NotchNook and Alcove moments. ROQA already has script progress and device alerts, so this would reuse them. Not in the no list. Check that it needs no polling. | M | none |
| 18 | The hover preview shows only the newest window of a group (left-over list). | DockDoor's whole pitch is "see every window"; uBar and ObjectDock Plus do it too. On a dock it's the most expected missing piece. | M | none |
| 19 | No way to bring back an auto-hidden dock or notch other than the edge strip (hotkeys exist but are off by default, and the notch's own hotkey is Ctrl+Alt+N). | If the edge strip fails (point 3), there is no visible fallback. Show a tray icon, or let a dock right-click reach "Show dock". | S | none |
| 20 | Settings has no import or export (left-over list). | The owner's store holds pins, prayer fine-tune and limits. Moving PCs or resetting loses them, and "Reset to defaults" is one click away. | S | none |

## Checked and fine
- Sleep and resume, display changes, DPI changes, Explorer restarts, session lock and device loss all have handlers (`native_window.rs`, `graphics.rs`).
- Copies a password manager marks private are kept out of the shelf (`clipboard_os.rs`).
- Two monitors: each display gets its own edge sliders and dock edge; the General page lists both displays.
- Brand: icons, capsule shapes, type and voice in Settings match BRAND.md, apart from items 9 and 10.

## Not tested
Real sleep and resume, unplugging a monitor, mixed DPI (both screens are 100 %), first run on a clean profile, every Settings row, the timer, reminders and shelf flows end to end (the fixture capture stopped early).

Sources: [NotchNook](https://lo.cafe/notchnook), [Alcove review](https://www.macstories.net/?p=188489), [Boring Notch](https://github.com/TheBoredTeam/boring.notch), [DockDoor](https://dockdoor.net/), [ObjectDock 3](https://www.thurrott.com/windows/windows-10/299793/stardock-releases-objectdock-3-for-windows-10-and-11), [uBar](https://alternativeto.net/feature/taskbar/).
