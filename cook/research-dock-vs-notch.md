# Dock vs notch: what belongs where (research, 2026-11-23)

Untracked scratch note. Delete once acted on (AGENTS.md docs rule).

## The rule the research points to

- **Top (notch) = status and live activities.** Apple: the Dynamic Island's compact state shows only the most critical live info; it is for things the person started and is tracking (timer, media, delivery), opened on demand or on an alerting update. Always-relevant info is a widget, not a live activity. The macOS menu bar's right side holds status and hardware/network extras.
- **Bottom (dock) = places you go and things you launch.** The macOS Dock holds apps, folder stacks and Trash; dock menus carry only a few high-value commands. The Windows taskbar holds Start, Search, Task View, apps and the tray corner (Quick Settings: volume, Wi-Fi, Bluetooth, power).
- **Nusa** (the reference) is notch-only: focus timer, water, to-do, media, file shelf, mascot. It has no dock, so it gives no guidance on the split beyond "live things live at the top".

Short form for ROQA: **notch = what is happening now; dock = where you go next.** Windows' own corner controls stay at the bottom because that is where Windows users reach for them.

## Inventory and verdict

Scoring: Glance = readable without opening; Freq = how often used; Life = persistent or transient; Reach = pointer travel from where the item is used.

### Notch (top)

| Item | Glance | Freq | Life | Verdict | Reason |
|---|---|---|---|---|---|
| Clock and date (collapsed pill) | high | constant | persistent | **Keep** | The one always-on reading; same spot as macOS and phone status bars. |
| Mascot eyes | high | ambient | persistent | **Keep** | It is the brand (blur test); only the notch can carry it. |
| Media tab (art, controls, seek, volume, player chips) | high | daily | live activity | **Keep** | Textbook live activity: started by you, tracked until it stops. |
| Timer tab (timers, Pomodoro, stopwatch, collapsed ring) | high | daily | live activity | **Keep** | Live activity with a glanceable collapsed ring. |
| Reminders (remind box, rows, snooze) | medium | weekly | transient | **Keep** | Time-based alerts belong with the timer and the notice row. |
| Weather tab | medium | daily | persistent | **Keep** | A glance reading near the clock; hourly detail needs the panel. |
| Prayer tab and adhan alert | high | 5x daily | alert + persistent | **Keep** | Time-of-day alert; sits next to the clock it depends on. |
| Shelf tab (files) | low | weekly | session | **Keep** | The notch is the drop target; a drag to the top is a clear gesture. |
| Clipboard (Shelf tab switch) | low | daily | session | **Keep** | Same "held for now" job as the shelf; never saved to disk. |
| Limits tab (off by default) | medium | daily for agent users | persistent | **Keep** (see move 2) | Status reading; fits the top. Could be a widget cell instead of a tab. |
| Widgets tab: week calendar, battery, memory, disk, CPU, network, next prayer, water, weather | high | glance | persistent | **Keep** | Apple's split exactly: always-relevant info is a widget. Stats sample only while shown. |
| Notice row: power, Caps Lock / language, Windows notification peek, device low battery, water, agent asks and progress | high | many per day | transient | **Keep** | Transient alerts are what the island is for; the dock never alerts. |
| Gear (Settings) and refresh | low | rare | persistent | **Keep** | Small, in the panel footer; out of the way. |

### Dock (bottom)

| Item | Glance | Freq | Life | Verdict | Reason |
|---|---|---|---|---|---|
| App slots: pins, running groups, window cycling | high | constant | persistent | **Keep** | The dock's core job; wide row needs the bottom edge. |
| Notification badges on icons | high | constant | persistent | **Keep** | A badge belongs on the app it is about. |
| Live previews and close button | medium | daily | transient | **Keep** | Tied to the slot under the pointer. |
| Context menu (new window, pin, close) | low | daily | transient | **Keep** | Few, high-value commands, as Apple advises for dock menus. |
| Folder stacks (pinned folder, newest files) | medium | weekly | transient | **Keep** | A place you go: macOS Dock stacks. |
| Start (left click) and quick links (right click) | low | daily | persistent | **Keep** | Windows muscle memory is bottom-left; owner rules lock its behavior. |
| Search, Task View | low | daily | persistent | **Keep** | Windows taskbar buttons; launch actions, not status. |
| Show-desktop strip | low | daily | persistent | **Keep** | Windows puts it at the bottom end; whole-desktop action. |
| Quick popover (volume, Wi-Fi, Bluetooth, power mode) | low | daily | transient | **Keep** | Windows keeps these in the taskbar corner; moving them breaks reach and habit. |
| Places (Recycle Bin, drives) | low | weekly | transient | **Keep** | Places you go: the macOS Dock keeps Trash here too. |
| Tray (hidden icons) | low | daily | transient | **Keep** | Windows' own flyout, anchored to its Explorer window at the bottom. |
| Device batteries button (always shown) | medium | rare | persistent | **Move (see move 1)** | It is a status reading, not a launcher; the laptop battery and the device low-battery alert already live in the notch. |

### On neither surface

| Item | Verdict | Reason |
|---|---|---|
| Edge sliders (volume right, brightness left) | **Keep** | Edge gestures; they need neither bar. |
| Global shortcuts (Ctrl+Alt) | **Keep** | Keyboard path to both surfaces. |

### Overlaps checked, left as they are

- Volume shows in three places (media page, Quick popover, edge slider). Each is reached from a different context; none is a duplicate surface. Keep.
- Weather and next prayer show both as tabs and as widget cells. The tab is the detail, the cell the glance; the user picks the cells. Keep.

## Proposed moves, ranked by value over effort

1. **Device batteries: dock button into the notch.** Value medium, effort medium. Fold the list into the notch's Battery widget (or a row under it) and drop the always-on dock button. Then every battery reading and alert sits in one place, the top, and the dock holds only places and launchers. Risk: the widget cell is small, so a list of several devices needs a tall cell or the Widgets page; needs a design round before code.
2. **Limits: tab into a widget cell (optional).** Value low to medium, effort medium. It is a glance-only reading with no actions but refresh, which is the widget job; it would cut the tab rail from seven to six. Keep the tab if agent users want the full three-source table; owner's call.

Nothing else should move. The current split already follows the "now at the top, go next at the bottom" rule, and Windows' own corner controls rightly stay at the bottom.

## Sources

- Apple HIG, Live Activities: https://developer.apple.com/design/human-interface-guidelines/live-activities
- Apple, 10 questions with the Live Activities team: https://developer.apple.com/news/?id=qpqf1gru
- Apple HIG, The menu bar: https://developer.apple.com/design/human-interface-guidelines/the-menu-bar
- Apple HIG, Dock menus: https://developer.apple.com/design/human-interface-guidelines/dock-menus
- Bjango, Designing macOS menu bar extras: https://bjango.com/articles/designingmenubarextras/
- Microsoft, Customize the taskbar in Windows: https://support.microsoft.com/windows/0657a50f-0cc7-dbfd-ae6b-05020b195b07
- Thurrott, Windows 11 taskbar (Quick Settings): https://www.thurrott.com/books/windows-11-field-guide/desktop/294126/taskbar-23h2
- AlternativeTo, Nusa: https://alternativeto.net/software/nusa/about/
