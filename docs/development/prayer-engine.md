# Prayer engine (pure maths)

Branch `feat/prayer-engine` from `8524cbd`. No UI, network, host wiring or GUI run. Research: `artifacts/research/weather-prayer.md` (git-ignored).

## API (`src/prayer.rs`, written from the published solar equations, no copied code)
- `times(&Params, (y, m, d)) -> Option<[i32; 6]>`: Fajr, Sunrise, Dhuhr, Asr, Maghrib, Isha as seconds since local midnight (`Prayer as usize`). `Params` = lat, lon, UTC offset in minutes for that date (caller passes DST), `Method`, `hanafi` (Asr factor 2). `None` in polar day/night.
- Methods: Egyptian (19.5/17.5, default), MWL (18/17), Umm al-Qura (18.5, Isha +90 min), ISNA (15/15). High latitude: angle-based (angle/60 of the night) whenever Fajr/Isha is missing or beyond it.
- `next_prayer(&Params, ymd, now_secs) -> Option<Next>`: strictly after `now`; after Isha it is tomorrow's Fajr. `Next::hm()` gives the countdown rounded up. Sunrise counts as an event; the tab may skip it.
- `mod prayer` carries `#[allow(dead_code)]` with a shortcut comment until the tab wires it.

## Checks
- `verify.ps1` PASS: fmt, clippy `-D warnings`, 224 tests (+10), locked release. 921,600 B, delta 0 B (dead code is stripped, so the real cost is unmeasured until wired); SHA `2A1C87D2D64BCD3140D3AA1CC32F53D7A458E7DD87152B44FBB8B5A4EBC7B7D9`.
- Reference: Aladhan API output fetched 2026-11-12 (URLs in test comments), within 2 min for Cairo (Egyptian, Shafi'i and Hanafi, and 15 Jul at UTC+3), Mecca (Umm al-Qura), London (MWL), New York (ISNA) and London 21 Jun (high-latitude fallback). Cairo 12 Nov: ours 04:48:13 06:17:34 11:39:11 14:39:22 17:00:29 18:20:26, reference 04:48 06:18 11:39 14:40 17:00 18:20.
- London 12 Nov Asr is 13:51:44 against the reference 13:54 (136 s, bounded at 3 min in the test): the sun is 15 degrees up and the reference's Asr rule is unpublished; both declination readings give 13:51.

## NOT RUN / deferred
- UNVERIFIED against official tables (Egyptian Survey Authority, Umm al-Qura calendar, London Central Mosque); Aladhan is another calculator. Reviews paused (REVIEW NOT RUN).
- gstack `/verify` not run: it would use the global toolchain, not the pinned one; `verify.ps1` is the gate.
- Shortcut: tomorrow's Fajr uses today's UTC offset (an hour off on a DST-change night). Not done: Ramadan Umm al-Qura Isha (120 min), polar fallback, Hijri date, location, UI.
