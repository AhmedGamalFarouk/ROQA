# Features: Weather tab (W1)

Branch `feat/weather` from `59bd345`. No native/GUI run, no real network call. Design: `artifacts/design-mascot/variants.html` (git-ignored).

## Layout used
- **J "Hero"** over K "Strip": condition picture, big temperature and H/L + condition at the left, the next hours as columns at the right, attribution as the footer. It is the one that fits the plain 320 x 80 panel (3 hour columns; 4 in the 360 x 120 media panel). The city replaces the page title; the clock stays top right.

## What changed
- `weather.rs` (pure): ~35-line JSON scanner (no allocation beyond the result), WMO code to 8 icons, request paths, `Report`/`Place` store lines (`wxplace=`, `wxdata=`), `Feed` (refresh rules and back-off), `View` (page text, UIA name "Cairo, 24 degrees, clear"). No new crates.
- `weather_os.rs`: one short-lived thread per job; WinHTTP over OS TLS (resolve/connect/send/receive each 8 s, 64 KB cap) and WinRT `Geolocator`; the answer waits in a channel and a thread message wakes the host (`brightness.rs` pattern). Coordinates are never logged; they are only in the store.
- Settings: city box, "Detect location automatically" (off), units Auto/C/F (Auto: F for US, LR, MM, BS, BZ, KY, PW, FM, MH). `Tab::Weather` is now available.
- Location order: device (when on), a held place, the typed city; none gives "Set your city in Settings". Nothing is requested until a city is set or detection is on. Detection runs once per session, per setting change, and again only while a city is the fallback.
- Refresh: on opening the page when data is over 60 min old; otherwise a background fetch when over 3 h old, only with a notch visible (not fullscreen-hidden), the user not idle 10 min and the tab on. No timer of its own: the minute clock wake runs it. Failure backs off 15 min, then 1 h; stale data shows with its age. Cache in the store for an instant start.
- Attribution "Weather by Open-Meteo.com" (CC BY 4.0) in the page footer.

## Checks
- verify.ps1: fmt, clippy -D warnings, 268 tests (scanner fixture, malformed/missing, WMO, staleness, open/background/hidden/idle/off, back-off, geocode, source order, nothing-before-configured), locked release.
- 1,070,080 B (+10.46% over 968,704); ratchet tripped, baseline raised (`raise-reason`, `approved-by: user (weather batch) 2026-11-12`). SHA `7D9BD380FC9571076FC1C496D46A2DFAFCDFFA817BE45B682A6564B284F4DE7A`.

## NOT RUN / deferred
- A real fetch (field names follow the Open-Meteo docs, UNVERIFIED live), the Geolocator call and its Windows privacy behaviour, offline/proxy behaviour, drawing of the page and icons, UIA read, settings layout on small screens (it grew by two rows). Reviews paused (REVIEW NOT RUN).
- Deferred: eyes "rain" mood (M1), reverse-geocoded name for a detected place (shown as "Current location").
