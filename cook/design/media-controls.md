# Notch media controls

[Classic mock: playing and paused](media-controls/classic-states.png). Geometry is in DIPs relative to the notch body. The expanded row is the visual refresh; compact hover is an optional interaction proposal and is not implemented.

## Expanded row

Preserve hit boxes: previous `(108,84,32,28)`, play/pause `(146,84,48,28)`, next `(200,84,32,28)`, mute `(244,88,24,20)`. Center glyphs in their boxes. Transport capsule radius `14`, no border. No changes to volume, progress, settings, cover, player chips or mode chips.

- Play/pause is the only accent button: theme accent fill (`#FFB454` Classic, `#9A5200` Light) with existing ink-on-accent glyph. Playing shows Pause; paused shows Play.
- Previous/next use palette ink and a 12% ink capsule before existing theme/reveal adjustment, not the draft's 18%. Mute stays a bare 18 DIP speaker/low/muted glyph.
- Disabled transport uses neutral capsule and ink at the existing `0.30` factor, rather than dimmed accent. Apply row reveal once and retain system disabled ink in high contrast. Keep expanded interaction and motion unchanged.

## Glyphs

20-unit paths centered in a 20 DIP box, 1.75 DIP stroke, round caps and joins, no fill. To use the 24-unit grid multiply coordinates by `1.2`. Preserve fractional stroke and centering without retuning shared glyphs.

| Icon | Path |
| --- | --- |
| Play | `M6.75 4.75 L15.25 10 L6.75 15.25 Z` |
| Pause | `M5.75 4.75 H8.75 V15.25 H5.75 Z M11.25 4.75 H14.25 V15.25 H11.25 Z` |
| Previous | `M4.25 4.75 V15.25 M15.75 4.75 L7.75 10 L15.75 15.25 Z` |
| Next | `M15.75 4.75 V15.25 M4.25 4.75 L12.25 10 L4.25 15.25 Z` |

Play sits 1 DIP right of geometric centering to balance the wider left edge. No filled transport exception. Preserve the media tab, passive cover fallback and state badge, including their existing tracing if shared with transport today.

## Compact hover toggle

The checkout draws a passive badge, not a dedicated compact hover action. Use the existing compact cover square as the hover/click target, not the badge. While hovered and enabled, replace the cover art/fallback and badge with a 20 × 20 circle at `(10,5)` at rest; follow the existing cover bounds for vertical centering. Classic: `#26262A` surface, `#DCE1E8` ink. Other themes use equivalent raised surface and normal glyph ink. No accent.

Center a 16 DIP glyph at `(20,15)`, 1.40 DIP stroke. Use the Play master or compact Pause `M7 4.75 V15.25 M13 4.75 V15.25`; the latter avoids tiny closed counters. Outside hover or when disabled, leave the passive cover intact.

Route the compact cover target before hover-to-expand so it stays clickable. One click invokes existing play/pause routing, without opening the panel or launching the player. Leaving the cover restores normal hover behavior. Never extend compact hit testing into the expanded panel; its cover still opens eligible players. Reuse existing pointer events and openness; no new delay, timer or curve. This target routing is the only proposed behavior change.

## Checks

- At 100%, 125% and 150% DPI: expanded Pause has two clear counters, compact Pause has two distinct strokes, and skip triangles stay separate from bars.
- Check playing and paused, with and without art, unavailable controls, Classic, Light and system high contrast.
- On the second monitor, confirm one compact cover click toggles once without expanding, moving to the rest of the pill still opens it, and expanded cover, mute, seek and volume are unchanged.
