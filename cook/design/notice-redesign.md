# Notch notifications and reminders

## Decision

“The notification or reminder should not open up the whole, maximized notch.” If the notch is closed, an admitted bell notification or due reminder uses a compact extension of the same top-attached pill. Keep the existing closed glance above one row. No page, tabs, rail, settings, or second window. If the user has opened the notch, use the footer below the current page without switching tabs. Other notice kinds keep their current behavior.

| Compact state | Classic | Light |
| --- | --- | --- |
| Bell notification | [Classic](notice-redesign/compact-classic-notification.png) | [Light](notice-redesign/compact-light-notification.png) |
| Due reminder | [Classic](notice-redesign/compact-classic-reminder.png) | [Light](notice-redesign/compact-light-reminder.png) |

Static design stills, 440 × 104 px at 96 DPI, with a default clock and synthetic copy. The earlier `classic-{short,long,reminder}.png` and `light-{short,long,reminder}.png` remain references for the **intentionally open** footer only. Judge compact and open states against `cook/RUBRIC.md`; the stills do not establish native motion or an 8.5 score.

## Compact geometry

Settled dimensions in DIP at default notch size, from the centered body's left edge and monitor top. Grow the closed silhouette around its existing glance in the same HWND. Respect the notch-size setting on the existing glance/open target, not by shrinking notice type. Clamp the notice to available client width; trim copy before touching the action or type size.

| Part | Bounds and styling |
| --- | --- |
| Body | 296 × 64, radius 32, 10-DIP closed top flares. Center like the rest pill. The original 150 × 30 glance stays centered horizontally at the top, clock centered at y=14. Keep alternate glance content; if it needs more height, place the row below it. Reuse the body's shadow. |
| Row | `(10, 28, 276, 30)`, radius 15. Clip paint and hit regions to the visible shape, including while growing or closing. While compact, only the original glance hit shape above the row can open the notch (the upper 28 DIP at default size). The row and surrounding apron never extend that target. |
| Icon and copy | Icon slot `(20, 35, 16, 16)`: existing bell or approved clock, neutral, 14-DIP optical size, round 1.75-DIP stroke. Leading-aligned Segoe UI Variable starts at x=44. Bell: 232 DIP, 13/400, one centered line or two centered as a block with 14-DIP step. Use the existing flattened copy in source order; do not infer a title. Wrap by word and end-ellipsize on line two; respect bidi and cluster boundaries. |
| Reminder | “Reminder” 12/600 and label 12/400 at y=36 and 50, with 130 DIP for the label and an 8-DIP gap. Ellipsize the label first, never Snooze. No label: draw “Reminder” once, vertically centered. “Snooze 10 min” 12/600 in a 92 × 24 capsule at `(182, 31)`, radius 12; the drawn capsule is the hit region. Keep the existing retry message in the label slot. |

Use existing theme materials; no new palette or blur. Mock body/row: Classic `#000000` / `#141416`, Light `#ECEEF2` / `#DDE0E6`. Primary/secondary ink: Classic `#FFFFFF` / `#B7B7BE`, Light `#14161A` / `#494D55`. Snooze: `#FFB454` with `#14161A`, Light `#9A5200` with `#FFFFFF`. Keep settled text contrast ≥4.5:1. Snooze is the only accent action.

## Transitions and input

| Event | Expected behavior |
| --- | --- |
| Arrival | Closed stays closed and reveals only the compact row. Intentionally open stays open and reveals only the footer. An existing user open/close transition continues toward its target; arrival never invokes full alert opening for these two kinds. Geometry appearing under a stationary pointer does not count as opening intent. |
| Reading | Hovering the compact bell row keeps the existing notice hold; leaving starts the existing 2 s release delay. Reminder hover does not extend its timing. Moving within the row or apron never arms open dwell or reopens a closing panel. Refresh row hover on reveal, reflow and promotion without synthesizing opening intent. |
| Deliberate opening / closing | In hover mode, actually entering the original glance target starts the normal dwell (including moving up from the row); leaving that target before it fires cancels it. In click-to-open mode, only clicking the glance target opens. Preserve valid existing user intent and normal open-panel linger. Carry one notice across the transition: compact yields to footer at the panel-content visibility gate, with one matching hit layout per frame. On normal close with a notice still active, return to compact without restarting its timing. |
| Click / Snooze | Bell: click the row to dismiss after the existing 500 ms settle guard. Only the reminder capsule snoozes; its label is inert. Consume compact notice clicks before generic open/control input, including label and pre-settle clicks. A press and release spanning a change of notice or presentation cannot act on a replacement or fall through to open. Preserve success (“Snoozed for 10 min”, existing plain notice) and at-capacity retry; neither forces the compact notch open. |
| Queue / exit | Preserve admission, first-refused waiting slot, order and promotion. Replace the row on promotion; never stack. Compact-to-compact promotion holds the silhouette steady, invalidates the old action and refreshes hover against the replacement. Without a successor, retire the row and return to the unmodified rest pill, or remove only the footer if open. Promotion to an unrelated notice kind uses its original policy and layout. |

Keep the 4 s/7 s bell hold (split at 40 characters), existing reminder wait, sound, retry, expiry, and visibility policy. Full-screen suppression and explicit visibility still win. Preserve the existing 5-DIP rise/fade, 240 ms `slide`, in-app fast handling, and open/close springs. Drive growth with the same notice tween; retiring visuals can fade but their actions stop immediately. No new timers, windows, settings, history, or disk writes.

## Expanded footer and Rust handoff

- Intentionally open: preserve the earlier footer with 24-DIP side insets, 36-DIP height, radius 16, top at page base minus 8 DIP (plus 4 DIP on playing Media), and bottom clearance 10 DIP. Leave at least 8 DIP after page content and hits; move the footer down if needed. At width 360 and base 128, use `(24, 120, 312, 36)`. Keep 12-DIP insets, 16-DIP line step and the 92 × 24 Snooze capsule; use a high-contrast neutral capsule if the page already has a filled accent primary. Move only the bottom edge and rail.
- Read `.agents/skills/rust-best-practices/SKILL.md`. Reuse the existing notice kind and reveal, one geometry calculation in `src/layout.rs` for paint and hit regions, existing row drawing in `src/graphics.rs` and the shared notice input/hover paths in `src/host/`. Gate full-open at activation and promotion, not in rendering. Keep the fixed HWND/buffer and current lifecycle in `src/power.rs` and the host. No extra poller or state machine.
- Verify stationary pointer on arrival/promotion, hover and click-to-open, one Snooze per click, stale-click rejection, queue/expiry return, retry, full screen, fast mode and fractional DPI. Check long/bidi copy, playing Media and tall Widgets/Timer. Use the second monitor for native checks when available; run `scripts\verify.ps1` and report the exe byte delta.
