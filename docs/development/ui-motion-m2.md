# U3: U2 leftovers and motion (M2)

Source: `docs/development/ui-council-fixes.md` (root, untracked), `ui-council-u2.md` (Not-done column). Branch `feat/u3-motion`. Size: U2 ended at 1,142,272 B; the user approved a one-time baseline raise to 1,152,000 B (limit 1,175,040 B, `scripts/size-baseline.txt`, `raise-reason: U3 leftovers + motion`). Hard cap for U3: about 1,152,000 B total. Decision 2026-11-13: no keyboard, focus-ring or UI Automation work in U3 (all of it is removed in the follow-up K1); existing keyboard/UIA code is untouched.

## Package A: leftovers

- **Limits:** rows at a 17 DIP pitch with 10.5 pt names and percentages (were 13 DIP, 9 pt), 10 pt reset times, 9.5 pt column labels, 8.5 pt "as of". The panel grows for the page: `layout::limits_height(rows)` (38 + 17 rows + 22 = 111 DIP) through a page-height override (`layout::set_page_height`, set by the host at the top of each notch paint, like the pill size), so the body, window region, hit tests, gear and handle all follow it. Other pages keep 80/120 DIP.
- **Timer:** the idle page shows "Start a timer" with four outlined buttons 5, 10, 15, 25 min (`timer::PRESETS`, `layout::page_button`). After a timer finishes the page keeps "Timer finished" in the accent colour with the same buttons. A click starts that many minutes when none runs.
- **Set city:** the empty Weather and Prayer pages draw a "Set city" outlined button under the message (`tab_art::pill`); it opens Settings. The message line no longer takes the click.
- **Tab handle:** the active tab shows its name beside its icon when the handle is open (active slot 76 DIP wide, buttons 26 DIP, handle 212 x 34 DIP for five tabs); the whole handle height is a click target, not only the buttons. Draw only: no UI Automation change.
- **Attached dock:** a full-width bar on the work area's bottom edge (slots left, taskbar buttons right, same top corner radius); the floating dock is unchanged.
- **Picker icon:** a new `Glyph::Apps` (3 x 3 small squares) replaces the Start panes on the dock picker slot.
- **Glass ears:** the glass layers of the notch body span the flared ears too, so they get the body's sheen instead of a clamped gradient edge. UNVERIFIED by eye (no native run).
- **Not done:** edge slider resting value/label (not cheap: the sliders are passive windows with no resting-state paint path; left out).
