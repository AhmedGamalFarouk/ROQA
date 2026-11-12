# B5 privacy indicator and M1 mascot moods (2026-11-12)

Branch `feat/f4-privacy-moods` on main b55fb90. No native/GUI run (NOT RUN: drawing, a real mic/camera session, UIA read, Settings fit).

## B5 privacy indicator
- **Source:** `HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\{microphone,webcam}\<app>` and `...\NonPackaged\<app>`: `LastUsedTimeStart != 0` and `LastUsedTimeStop == 0` is in use (`privacy::in_use`, pure, fixture tests). App names are read into a stack buffer and dropped; never stored, logged or exposed.
- **Event-driven:** one `RegNotifyChangeKeyValue` (subtree, last-set) on the `ConsentStore` key signals one auto-reset event. The event is the last handle of the event loop's existing `MsgWaitForMultipleObjectsEx` set; on signal the key is re-armed, then read once. At start one read. No thread, no timer, no polling; nothing runs while nothing changes.
- **Setting:** Settings "Privacy indicator" (`privacy=`, default ON), applied live. Off drops the watch (key and event closed, handle removed from the wait set next pass) and the dots fade out.
- **Drawing:** orange mic dot and green cam dot (6 DIP) at the right of the pill and the open panel's header row, drawn in the notch text pass. Fade is three 60 ms steps, timed only while a dot is changing (`privacy::Dots`).
- **UIA:** the notch root's notice reads "Microphone in use", "Camera in use" or both, as a polite live notice (a power notice wins while one shows). No app names.
- **Limit:** the pill's right edge may meet the media pill's visualizer; unchecked visually (NOT RUN).

## M1 mascot moods
- `eyes::Mood::Adhan`: half-lidded (both lids 25%), stepped glow (three opacity pulses, 11 paints, about 4 s); facts `adhan` is true for the reminder's 8 s notice (`PrayerRun::adhan_until`) and outranks every other mood.
- `eyes::Mood::Rain`: droopy (top lid 40%, sitting 2 DIP low), one 6-step show; fact `rain` is true while the Weather page is the active page and the cached report's code is rain or storm; ranks after alerts, before music and sleep.
- Per-tab pairing: Media playing = happy, Timer finished = surprised, Prayer reminder = adhan, Weather rain = rain; other tabs keep idle, sleepy and wake. A mood shows once on entry, then the pill returns to the time; idle paint rate is unchanged.
- **Limit:** the eyes fade out as the panel opens, so a mood that begins while the panel is open (a tab switch, a reminder that opens the notch) plays unseen; it is visible when it begins on a collapsed pill (data refresh, start-up). Unit tests cover selection and the shows.
