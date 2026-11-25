# Dock widgets and command center (design spec, 2026-11-24)

Owner ask (2026-11-24): widgets on the dock like LumaBar's, and a command center for Bluetooth, Wi-Fi, airplane mode and the like. This reopens two lines of `cook/research-lumabar-fit.md` ("widget picker", "Quick Toggles use native entry points"). The owner's request overrides them; the limits below keep the old reasons alive.
Mock: `widgets-command-center/mock.html?v=<view>`. Stills (2x, headless Chrome, sample data 24 Nov 2026): `dock.png`, `states.png`, `pops.png`, `popthemes.png`, `center.png`, `pages.png`, `open.png`. Not seen live. No critic round yet.

## Owner decisions (2026-11-24, after `cook/research-command-center-workarounds.md`). These win over anything below.
1. **Airplane:** try the direct switch through Windows' private radio interface (`CLSID_RadioManagementAPI`, `IRadioManager::SetSystemRadioState`), as a spike first. Read the state before setting, verify by reading it back and by Windows' own icon, and fall back to opening `ms-settings:network-airplanemode` on any failure. The old "all radios off equals Airplane" rule and the forced-Off Wi-Fi and Bluetooth reading are withdrawn: Airplane shows Windows' own state, and Wi-Fi and Bluetooth keep showing their own radios. Private ABI: it may break on a Windows update; the fallback keeps the tile working.
2. **Hotspot:** the tile is a normal enabled action that opens `ms-settings:network-mobilehotspot`. No direct toggle, no spike, no dimmed tile.
3. **Bluetooth:** read-only list with battery and a Windows settings link. No connect or disconnect.
4. **Do Not Disturb, Night light, mic picker:** left out. No links.
5. **Widgets move from the notch to the dock.** The owner waived the follow-up mock. Settings sets the visible cap from 1 to 6 (default 3); enabled tiles beyond the cap stay selected but hidden, and narrow docks omit tiles that do not fit. The notch's two widget slots are removed; the dock tiles replace them (table below). The notch keeps its tabs, media, glance line and clock.

## What LumaBar actually does (read 2026-11-24, lumabar.xyz home, /widgets, /docs)
- Widgets are tiles in the bar. Right-click the dock, "Widgets...", pick from 109 tiles in 13 categories, drag to reorder. Some tiles open a card on click.
- Quick Toggles is one tile: "Wi-Fi, Bluetooth, mobile hotspot and airplane mode". Brightness and Audio Output are separate tiles.
- The site does not state tile size, popover layout, or whether hover shows anything. Do not claim parity beyond the above.

## What ROQA already has (checked in code)
- Notch widget slots: 9 kinds (`src/widgets.rs`), shown inside the open notch only.
- A "Quick" dock button (sliders glyph) opening a plain list menu: sound, Wi-Fi, Bluetooth, power mode, keep awake, theme, lock, sleep (`src/host/menus.rs` quick rows, `src/quick.rs` radios).
- Volume and brightness buttons with slider popups, battery, Places, Limits, Moon, tray.
- Missing: airplane mode, hotspot, network and device lists, anything glanceable on the dock itself.

## Decision
1. **Command center** replaces the Quick list. Same button, same place, a richer popover. This is the main value.
2. **Dock widgets** are small tiles that show one live reading. Adjustable limit of 1?6, default 3, picked from a short fixed list in Settings. Not a catalog, not free-form, no account widgets.

## 1. Command center
Opens above the Quick button through the shared owned placement, 14 DIP gap, hover (`preview::DELAY`) or click, closes 180 ms after the pointer leaves, holds the dock open while shown (same as the volume popup). Colours follow the five themes.
- **Popover:** 308 wide, pad 8, radius 28 (capsule grammar: tile radius 24 = height/2).
- **Six tiles, two columns, 48 high:** Wi-Fi, Bluetooth, Airplane, Hotspot, Keep awake, Theme. Glyph 20 (stroke 1.75), name 13/500, state line 11 at ink .70.
  - On: ink 14% fill, full ink glyph, a 6 DIP Lamp dot at the right (the selected indicator). Off: ink 7% fill, glyph at 65%. No accent fill, no outline.
  - Click a tile flips it. Wi-Fi and Bluetooth, when on, show a chevron (32 wide target) that opens their page.
  - Hover: the existing 22% Lamp capsule.
- **Two slider rows (40 high):** volume (master) and brightness (the dock's own monitor). These are the existing mixer-row sliders, same press, drag, wheel and mute rules. Other monitors stay in the Brightness button's popup.
- **Footer:** Power mode capsule (click cycles, as today), Lock, Sleep as 40 round capsules.
- **Pages (slide curve):** the body swaps for a page with a back button, title and an on/off switch.
  - Wi-Fi: "Saved networks in range", the connected one first with "Connected", others "Saved". Click connects (known profile). Last row "More networks" opens Windows' own Wi-Fi list.
  - Bluetooth: "Paired devices" with connected state and battery percent where Windows reports it (reuses device batteries). Rows are informational: Windows has no public connect or disconnect call. Last row "Add a device" opens Windows Bluetooth settings.
- **Copy:** Wi-Fi, Bluetooth, Airplane, Hotspot, Keep awake, Theme, Dark, Light, Off, On, "2 devices", "Home 5G", "Saved networks in range", "Paired devices", "More networks", "Add a device", "Open settings". Nothing else new.
- Replaces rows Mute/Volume up/Volume down (now the slider), keeps Power, Keep awake, Theme, Lock, Sleep.

## 2. Dock widgets
- **Where:** right cluster, left of the Command center button. A tile is a 36 high capsule in the 44 cell: glyph 20, reading 12.5/500 tabular, pad 10/12. Rest: no fill, ink. Hover: 22% Lamp capsule. No tooltip. Click or hover opens its popup (same rules as the Limits and Moon popups).
- **Moved from the notch (owner, 2026-11-24).** Each of the nine notch widget kinds goes to the dock or is folded in: Week calendar becomes Calendar; Next prayer becomes Prayer; Weather stays Weather; Water becomes Water; Memory, Disk, Processor and Network speed fold into the one System tile (open-only sampling, so the notch's always-on 1 s sampling goes away); Battery already has its dock button, so its notch slot is simply removed. The notch's widget slots, their Settings rows and `WidgetConfig` are deleted; the notch panel uses the space for its tabs. The owner waived the follow-up mock.
- **Fixed list, six kinds** (all event or minute driven; nothing new polls):

| Tile | Reading | Popup | Cost |
|---|---|---|---|
| Weather | "29°" | now plus next three hours | existing weather data, existing fetch policy |
| Prayer | "Asr 12m" | five prayers, Lamp sun on the next | existing minute clock and `prayer_tab` |
| Calendar | "Tue 24" | month grid, Lamp today marker | `widgets::month_grid`, minute clock |
| Timer | "18 min", shown only while a countdown runs | time left, bar, Pause, Add 5 min | existing timer; appears after the chosen tiles |
| Water | "3 of 8" | the glasses today, "Add a glass" and "Remove one" (existing water logic) | existing, no timer |
| System | glyph only | CPU, memory, disk, network speed | samples at 1 s **only while the popup is open** (`widgets::Stats`), nothing at rest |

- **Settings:** a "Dock widgets" group, one switch per kind and an adjustable maximum of 1?6 visible widgets. Order is the list order above; no drag. Store keys: one line in the store, settings only. Default: none, so the dock is unchanged until chosen.
- **Not doing:** live CPU or network speed on the tile (needs an always-on 1 s timer), world clock tile (lives in `world-time.md`), finance, social, account or developer feeds, free-form widget wall, drag ordering, per-widget settings.

## What Windows allows (research, 2026-11-24)
| Part | Route | Verdict |
|---|---|---|
| Wi-Fi, Bluetooth on or off | `Radio.SetStateAsync`, already in `quick.rs` | works today |
| Airplane | Docs have no airplane call; the sample enumerates radios and sets each ([Radio.SetStateAsync](https://learn.microsoft.com/en-us/uwp/api/windows.devices.radios.radio.setstateasync?view=winrt-26100)). Tile on = every radio off; turning off restores the ones that were on, held in memory only. | works as "all radios off"; **check on hardware** that Windows' own switch follows |
| Hotspot | `NetworkOperatorTetheringManager`; docs list the `wiFiControl` capability and say `CreateFromConnectionProfile` throws without it ([docs](https://learn.microsoft.com/en-us/uwp/api/windows.networking.networkoperators.networkoperatortetheringmanager.createfromconnectionprofile)). ROQA is unpackaged. | **unproven**: one-hour spike first; if it throws, the tile opens `ms-settings:network-mobilehotspot` and says "Open settings" |
| Wi-Fi list | `wlanapi`: list, `WlanConnect` with a saved profile, no UI ([WlanConnect](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/nf-wlanapi-wlanconnect)); scan and notifications only while the page is open | works for saved networks; new networks and passwords go to Windows' list |
| Bluetooth list | `Devices_Enumeration` already enabled; batteries already read | list works; connect and disconnect have no public call |
| Do not disturb, Night light | no public API | left out |

## States
| State | Result |
|---|---|
| No Wi-Fi or Bluetooth radio | that tile is absent (as the list rows are today) |
| Radio access refused or call times out | tile unchanged, notice "Still busy. Try again." (existing) |
| Airplane on | Wi-Fi and Bluetooth read Off, no chevrons |
| Hotspot spike failed | dimmed tile, "Open settings" |
| Wi-Fi off | page shows the switch off and no rows |
| No saved network in range | one row "No networks in range" |
| Full-screen app, several docks | unchanged: dock hidden; each dock opens its own popover |
| Three tiles chosen on a narrow monitor | existing dock width rules apply; to check in the live pass |

## Touch points
`src/dock.rs` (widget tiles, order, width), `src/quick.rs` (airplane, hotspot or deep link, radios list), a new small `src/wifi_os.rs` (wlanapi worker; `Win32_NetworkManagement_WiFi` feature), `src/host/menus.rs` and `src/dock_menu.rs` (popover with tiles, sliders, pages; today a list), `src/widgets.rs` (popup readings), `src/settings_window/` (Dock widgets group), `src/store.rs` (one key). `host.rs` and `graphics.rs` must not grow; the old Quick list code pays for it. README feature list changes in the same commit.

## Order of work (each step passes verify.ps1; revised after the owner decisions)
1. Airplane spike (throwaway, not in the tree): standard user, read state, set, read back, Windows icon agrees, HRESULTs logged. Pass: direct switch. Fail: the tile opens Settings.
2. Command center popover: six tiles, sliders, footer. Hotspot and, on spike failure, Airplane open Windows settings. Old list deleted.
3. Wi-Fi page (wlanapi worker, location-denied path opens Windows' list in the same slice), then the read-only Bluetooth page.
4. Dock widget tiles and popups: Calendar, Prayer, Weather, Water first; Timer, System after.
5. Delete the notch slots, their Settings rows and `WidgetConfig`; add the Dock widgets group and store key. Old stored slot choices are ignored, not migrated.
6. Second-monitor live check: both popovers, hover and click, airplane round trip, wide and narrow dock, themes, notch with no slots.

## Measured
Nothing yet: no build, no byte delta. Layout numbers above are from the mock, not from code.

## Open
1. Notch without widget slots: no additional mock; keep the remaining tabs.
2. Dock widget cap: adjustable in Settings, default 3.
3. The Limits table above (Airplane, Hotspot, Bluetooth, Do not disturb rows) is superseded by the owner decisions at the top.
