# Feature ideas for ROQA (research, 2026-11-23)

Scratch research for the Chief. Untracked; delete once acted on.
Checked against `FEATURES_TO_ADD.md` (wish list, Wave B, NO lists), the README and the source, so nothing here is already listed, built or ruled out.
Already built and skipped here: clipboard history, attention badge, Keep on top per window, hide app from dock, groups, Recycle Bin and Places, privacy dots, Caps Lock pill, Wi-Fi and Bluetooth toggles, power profile, keep awake, brightness, device batteries, script progress pill, folder stack (Downloads).

Fit = fits AGENTS.md (event-driven, no idle timers or threads, small, brand, no tooltips, no UI Automation).

## Top 10 (ranked)

| # | Idea | What it does | Who wants it | Effort | Fit |
|---|------|--------------|--------------|--------|-----|
| 1 | Per-app volume | A row per app playing sound, each with its own slider, under the notch volume bar. | Anyone with music plus a call or game; EarTrumpet is one of the most installed Windows tray tools. | Medium | Yes. Core Audio session events tell ROQA when an app starts or stops sound; no polling. Not the "replace the Windows volume pop-up" item on the NO list. |
| 2 | Microphone mute | One click or a hotkey mutes the mic; a small accent dot stays in the notch while muted. | People on calls; PowerToys had this and users missed it when it was removed. | Small | Yes. Uses the mic's own mute-changed event and the existing hotkey list. Sits beside the privacy dots. |
| 3 | Wheel over a dock icon | Scrolling over an app's icon steps through its windows. | Taskbar power users; a top Windhawk mod does just this. | Small | Yes. Input only, reuses window cycling. |
| 4 | Hijri date | The Islamic date beside the normal date in the notch. | The owner and every Prayer tab user. | Small | Yes. Windows has the Umm al-Qura calendar built in; it changes once a day with the clock tick ROQA already has. |
| 5 | "Rain soon" pill | A short pill when rain is expected within the hour. | Weather tab users. | Small | Yes. Read from the forecast ROQA already fetches; no new timer. Voice: "Rain in 40 min". |
| 6 | Screenshot pill | After a screenshot, a small thumbnail pill; drag it to the Shelf or into an app. | Anyone who takes screenshots; NotchNook and Notchify users like the tray for this. | Medium | Yes, if the image stays in memory and is saved only when the user drags it out (store rule: never save copied things to disk). |
| 7 | `roqa notify "text"` | Scripts and terminals show a one-line pill ("Build done"). | Developers and agents; same idea as Raycast script commands. | Small | Yes. Extends the existing `progress` command and its pill. |
| 8 | Recently closed apps | The last two or three closed unpinned apps stay on the dock, faded, to reopen in one click. | Mac users (the macOS dock "recent apps" section). | Small to medium | Yes, if the list lives in memory only. Owner decides whether the dock may grow. |
| 9 | Pin any folder | Pin any folder as a dock stack, not only Downloads. | Folder-heavy users; the old Stacks docklet for RocketDock and ObjectDock. | Small | Yes. Reuses the folder stack. Check first that Places does not already do it. |
| 10 | Share from the Shelf | A Share button on a Shelf file opens the Windows share sheet (Nearby Share, mail, Phone Link). | NotchNook users who use AirDrop from the notch. | Medium | Likely. Windows offers the share sheet to plain desktop apps; test that it works without app packaging before building. |

## Smaller extras (not ranked)

- **Dock spacer**: a gap you can drop between pins to group them. Small. Fits. Seen in Winstep Nexus and the macOS dock.
- **Week number in the date**: an optional "Week 47" in the notch date. Small. Fits. The Windhawk clock mod is the second most installed mod (no seconds: that needs a 1 s timer).
- **Pin a clip**: keep one clipboard entry at the top of the list. Small. Fits only if pins stay in memory.
- **Window count dots**: a dot per open window under a running icon, up to three. Small. Fits. Check the dock first; it may already show this.

## Dropped (break a hard rule or cannot work)

- Text snippets that expand as you type (Raycast): needs a keyboard hook that sees every key.
- Seconds in the clock: needs a 1 s timer.
- Answer phone calls in the notch (DynamicLake): needs app packaging, like notification peeks.
- Night light toggle: Windows has no public switch for it.
- Space to preview a Shelf file (PowerToys Peek): large; owner can ask for it later.
- App names on hover in the dock: tooltips are ruled out.
- Wallpapers, tiling, multiple docks (Seelen UI, ObjectDock Plus): already on the NO list or far from ROQA's job.

## Sources

- Nusa: https://alternativeto.net/software/nusa/about/ , https://github.com/rama2555/nusa/releases
- Windows notch apps (Notchify, WinIsland, DynamicWin, Fly Island): https://www.windowscentral.com/software-apps/notchify-brings-macos-style-dynamic-island-flair-to-windows-11 , https://fttawa.github.io/WinIsland-X/ , https://alternativeto.net/software/winisland/about/
- DynamicLake (Mac; calls, glass styles): https://alternativeto.net/news/2025/5/dynamiclake-introduces-support-for-facetime-iphone-calls-in-the-dynamic-island-and-more
- Mac notch apps (NotchNook tray and AirDrop, Boring Notch, Alcove): https://www.macupdate.com/comparisons/notchnook-vs-alcove-whats-the-best-macbook-notch-app-in-2026 , https://alternativeto.net/software/the-boring-notch/about/
- Seelen UI and Winstep Nexus: https://seelen.io/apps/seelen-ui/customizable-shell , https://www.techspot.com/downloads/6897-winstep-nexus.html
- Windhawk most used mods (wheel cycling, clock): https://www.windowscentral.com/microsoft/windows-11/7-best-windhawk-mods-to-transform-your-desktop-on-windows-11 , https://windhawk.net/explore
- RocketDock and ObjectDock docklets and stacks: https://www.makeuseof.com/tag/rocketdock-stacks-docklet-not-just-a-dock-but-a-complete-desktop-organization-solution-windows/ , https://alternativeto.net/software/objectdock/about
- Raycast for Windows (script commands, clipboard, snippets): https://raycast.com/changelog/windows , https://raycast.com/blog/raycast-for-windows
- PowerToys (Peek, Light Switch, Command Palette): https://devblogs.microsoft.com/commandline/powertoys-0-97-is-here-a-big-command-palette-update-and-a-new-mouse-utility/
- EarTrumpet (per-app volume): https://github.com/File-New-Project/EarTrumpet
- Windows share sheet for desktop apps: https://learn.microsoft.com/windows/apps/develop/ui-input/display-ui-objects
