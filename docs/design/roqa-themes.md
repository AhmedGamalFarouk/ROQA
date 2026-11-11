# ROQA themes (design pass, PROVISIONAL)

Author: Claude Sonnet 5.5, 2026-11-11, on the user's request ("everything, notch body too; one theme is liquid glass"). Addendum to `roqa-visual-spec.md`, whose "Always dark" rule becomes the **Classic** theme and stays the default. Sizes, shapes, motion and layout do not change; a theme only swaps colours (and, for glass, adds a rim and a body alpha). Nothing here has been looked at on a real display: every look below is a design target, NOT RUN.

## Palette tokens (`src/theme.rs`)
`body` (notch body and the dock bar at rest), `dock_hover` (bar with the pointer in), `ink` (text, glyphs, bars, rings; each element keeps its own alpha), `on_ink` (glyph on an ink circle, badge circle), `glyph` (dock picker tiles), `tile` (icon-less group), `shadow` (notch shadow strength; Classic 0.9), `rim` (glass only: a 1 px stroke along the notch outline and the dock bar, bright at the top and fading toward the bottom).

| Token | Classic | Light | Midnight | Liquid glass |
|---|---|---|---|---|
| body | `#000` | `#F4F5F7` | `#0B1020` | `#161C28` at 82 % |
| dock hover | `#141416` | `#E8EAEE` | `#161E38` | `#161C28` at 90 % |
| ink | `#FFF` | `#14161A` | `#E8EEFF` | `#FFF` |
| on ink | `#000` | `#FFF` | `#0B1020` | `#0A0E16` |
| picker glyph | `#DCE1E8` | `#343840` | `#BECDFF` | `#EBF0FA` |
| tile | `#3A3A40` | `#CDD0D6` | `#283254` | white 16 % |
| shadow | 0.9 | 0.35 | 0.9 | 0 |
| rim (top to bottom) | none | none | none | white 55 % to 10 % |

- **Classic** is exactly today's look, and the only theme that blends with a hardware notch.
- **Light** inverts the roles: dark ink on a pale body, a soft shadow so the pill still reads on a white wallpaper.
- **Midnight** is Classic with a deep navy body and a cool ink, for a calmer dark.
- **Liquid glass** is a dark tinted pane at 82 % body alpha with a specular rim. Glass has no shadow: the shadow window sits behind the body and would show through it as mud.

## Liquid glass: what it is and is not
It is translucent tint plus the bright top rim, drawn in Direct2D on the existing premultiplied surfaces, so it costs one extra stroke per paint and no new resource, thread or wake. It is **not** a blur of what is behind it: that needs a DWM backdrop (`DWMWA_SYSTEMBACKDROP_TYPE`) whose behaviour with the notch's window region and the dock's transparent corners cannot be judged without a native run. That is the follow-up experiment, to be run by the user. The body alpha is chosen so text stays readable over a white wallpaper (test: primary 7:1, 60 % text 4:1 over the worst backdrop).

## Rules
- Contrast is a unit test over every theme at the worst backdrop (white for translucent bodies): primary ink at least 7:1 and the 60 % ink (date, status) at least 4:1.
- The meter's partial repaint refills the bar slot with `body`. Every body is a single flat fill (the rim is a perimeter stroke), so the refill matches the full paint exactly.
- A theme change rebuilds the surfaces the way a text-scale change does (palette fixed at creation), once per change; no timer, no resident cost.
- The settings window and the window picker are standard Windows controls and follow the system theme, not ROQA's.
- High contrast mapping is still not implemented (known limit unchanged).
