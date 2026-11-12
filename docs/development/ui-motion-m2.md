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

## Package B: motion (M2)

One motion clock (`motion.rs`: `Tween`, `Curve`, `paced`), so animspeed (Fast) shortens every new animation too. Frames run only while a tween runs (`Surface::live` in `Desktop::animate`); nothing starts one at rest. Reduced motion: unchanged by decision 2026-11-09 (the Windows "Animation effects" setting is not consulted; the Settings animation speed is).

- **Open/close easing:** open spring 0.66 / 19 (was 0.65 / 18: about 6.3 % overshoot, peak near 220 ms, was 230); close 0.64 / 17 (was 0.61 / 16: dips about 7 % below the pill, was 9 %). Test bound for the close dip widened to 1.05 to 1.10. Tuned by numbers, NOT judged natively.
- **Tab switch:** while the panel is open, a switch of page slides the new page in 18 DIP from the side of travel (later tab from the right) and fades it in over 240 ms (`motion::page_switch`, `Surface::page_fx`, `Fx` in the scene, one `tab_art::faded` layer). Stagger: the title and clock lead, the content starts 25 % later. Closed or opening panels start nothing (the open reveal already fades the page).
- **Content cross-fade:** the page fade-in covers the shelf, timer, prayer, weather and limits content together. NOT done: a fade-out of the leaving page (its data is not kept) and fades for data that changes inside a page.
- **Notice:** the power/prayer notice rises 5 DIP and fades in (`Fx::notice`, `Surface::notice_fx`). NOT done: an exit animation (the text is gone when the notice ends).
- **Dock hover:** the hovered icon rises 4 DIP (was 3) and grows up to 10 % in whole pixels, on a new `POP` curve with about 10 % overshoot; the bar fill keeps the plain ease-out.
- **Eyes:** blink and look shows anticipate with a 50 ms stretch, squash for 90 ms and overshoot back with a 60 ms stretch (two more paints per show, same deadlines).
- **Settings toggle:** the thumb slides over 150 ms (`draw_toggle`: state and start tick kept in a window property; the toggle invalidates itself only while sliding, no timer). NOT done: the Settings page transition (the page is a GDI child set; a cross-fade needs a second surface), the focus-ring fade (keyboard work, out of scope by the K1 decision).
- **Idle proof:** `motion::tests::an_idle_notch_starts_no_frames` (same page or closed panel start no switch; a settled tween is not running); the existing `Desktop::animate` rule (live only while a tween runs) is unchanged.

NOT RUN: native/GUI. Feel, timing and the slide distance need a desktop-free native look.
