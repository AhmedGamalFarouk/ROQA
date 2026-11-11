# M5 themes: four palettes in settings, including Liquid glass

Author: Claude Sonnet 5.5 (Tier 2), branch `feat/themes`, base `2d4d97f` (S7). Requested by the user: "themes in the settings", scope "everything, notch body too", design first, one theme is liquid glass. REVIEW NOT RUN (paused by user 2026-11-11). Gate: `scripts/verify.ps1` PASS, **175 tests** (170 + 5). Release `roqa.exe` 999,424 B, SHA-256 `3CE2BD8E4230E4FD3C025265236E0A7786A08002AE519673FD0BE46DB7FB583A`; base 990,208 B, so **+9,216 B (+0.9 %)**. No new crate or `windows` feature.

## Design
`docs/design/roqa-themes.md` (the design, written first) is an addendum to the visual spec: palette tokens, the four themes, the glass rules. The spec's "always dark" rule is now the **Classic** theme and stays the default, so an upgrade changes nothing.

## What changed
- `src/theme.rs` (pure): `Theme` (Classic, Light, Midnight, Liquid glass), `Palette` (body, dock hover, ink, on-ink, picker glyph, tile, shadow strength, glass rim), `dock_fill` blend. Classic equals the old hard-coded colours (test).
- `src/graphics.rs`: every hard-coded colour (about 25 sites: `WHITE`, black body, dock fill, glyph, tile, badge and chip glyph) now reads the canvas's `Palette`, fixed at `Canvas::create`. Glass adds a 1 px gradient rim (notch outline, dock bar) and a 0.82-alpha body. Meter refill uses the body colour: every body is one flat fill, so the partial repaint still matches.
- `src/settings.rs`, `store.rs` lines: `theme=<key>` (store version stays 2; an old build ignores the line). Reset returns Classic.
- `src/settings_window.rs`: a "Theme" drop-down list (standard combo box, tab order, mnemonic H); applies on selection.
- `src/host.rs`: a theme change rebuilds the surfaces once, as a text-scale change does. No timer, thread or resident cost; the closed-window idle rule is unchanged.

## Tests (5)
Classic is the legacy look; key round trip and unknown names; contrast over the worst backdrop for every theme (primary ink 7:1, 60 % ink 4:1, on-ink 7:1); only glass is translucent with a rim and no shadow; dock fill blend. Settings tests extended for the theme line, clamp and reset.

## Defaults relied on
Mine: four themes; Classic default; the settings and picker windows keep the system theme; glass has no shadow (it would show through the body).

## NOT RUN
Every theme on a real display (no screenshot or capture was taken), the glass rim and body alpha over bright and dark wallpapers, the meter partial repaint under glass, a theme switch while a notch is open or animating, the dock under glass, multi-monitor and mixed DPI, text scale.

## Deferred
A true backdrop blur (DWM `DWMWA_SYSTEMBACKDROP_TYPE`; its behaviour with the notch region and the dock's transparent corners needs a native run), Follow Windows light/dark, per-monitor themes, high-contrast mapping, theming the settings and picker windows, an accent colour picker.
