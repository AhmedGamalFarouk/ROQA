# Nusa reference study (observed, not copied)

Author: `/root`, 2026-11-09. Purpose: describe Nusa's visual and motion language so ROQA can match its *feel* with its own implementation. Nusa publishes no design tokens. Nothing here is Nusa code or artwork, and none of it ships in ROQA.

## Sources

| Id | Source | Kind |
|---|---|---|
| S1 | Official site `https://rama2555.github.io/nusa/` (v1.2.0 page), fetched 2026-11-09 | The developer's own HTML/CSS mockup of the notch, with sizes written in dp, plus feature copy |
| S2 | Three screenshots on touchit.sk (27 Oct 2026), captioned "Zdroj: Nusa" | Collapsed clock pill, Pomodoro panel, music panel |
| S3 | AlternativeTo listing, Microsoft Store listing (title only) | Feature list: "minimal pill", mascot, size setting |
| S4 | Live captures of the installed Nusa on the author's machine (user-approved, desktop confirmed free) | `artifacts/design-review-1/nusa/` and `nusa-run2/`; measurements below |

S1 and S2 are stored for comparison only under the git-ignored `artifacts/nusa-reference/public/`. We read the public page's stylesheet to *measure* sizes and timings, the way one would measure a screenshot. No CSS, markup, image, font or sound was copied into ROQA. Nusa's binaries and resources were not opened.

Key: **M** = measured from a source (Sx), **E** = estimated by eye from screenshots.

## Notch: what Nusa does

### Shape and material

- **Shape (M, S1, S2).** Pure black (`#000`), flush with the top edge of the screen, with rounded bottom corners. The top corners flare outward into small **concave "ears"**: quarter-circle fillets that blend the body into the screen edge, like a hardware notch.
- **Sizes in dp (M, S1)**, as width × height, bottom radius, ear size:
  - Collapsed: 150 × 30, r 14, ears 10.
  - Compact live activity (charging, Bluetooth, notifications): 310 × 60, r 19, ears 13.
  - Full panel (music, timers, shelf): 420 × 140, r 32, ears 22. Lyrics: 520 × 140.
- **Shadow (M, S1).** A soft drop shadow under the body, offset about 14, blur about 34, spread −16, black at about 95% alpha. It's subtle and mostly hidden under the body. In the screenshots it's barely visible (E).
- **Material (M, S1, S2).** An opaque black body: no acrylic, no blur, no border.

### Type and content

- **Type (M, S1, S2).** "Segoe UI Variable Text", falling back to Segoe UI. 13 px white in the collapsed pill. The time is bold, and the date in the header is regular weight (S2: "Mon 26 Oct 2026 **15:46**").
- **Collapsed content (M, S2).** The time, bold and centred. A green progress ring sits on the left and audio level bars on the right when media plays. When nothing happens, the clock fades out and a mascot's "pair of eyes" appears, glowing blue (S1 copy plus `.eye` styling).
- **Expanded content (M, S2).** A header row with the date and the bold time, centred near the top. Below it are the page content (album art, title/artist, progress, controls) and page dots, where the active dot is a 15 px white pill. Secondary text is dimmer white. Control chips are `rgba(255,255,255,.12)` pills.

### Behaviour and motion

- **Behaviour (M, S1 copy).** "Hover the notch to open it. Move away and it closes." It hides for full-screen apps. "Draws only while something moves. When it's idle it uses no CPU at all."
- **Expand (M, S1).** Width and height animate over **0.62 s** with an overshooting ease, `cubic-bezier(0.34, 1.32, 0.5, 1)`. That curve peaks at about 1.034 (a 3.4 % overshoot: a single soft settle, not a bounce). The radius animates separately over 0.45 s with a standard ease.
- **Collapse (M, S1).** About **0.5 s** with `cubic-bezier(0.3, 0, 0.15, 1)`. The site's comment says it "shrinks without bounce".
- **Content (M, S1).** Header and dots fade in over 0.55 s after a **0.1 s delay**. They slide into place over 0.5 s with a strong ease-out, `cubic-bezier(0.2, 0.85, 0.25, 1)`, after a 0.05 s delay. The content appears *after* the shape has started opening, never before.

## Live capture of the installed Nusa (S4, measured)

Setup:

- **When and where:** 2026-11-09, 1920×1080 at 100 % scale (96 DPI). The user approved the capture and confirmed the desktop was free.
- **Capture:** Nusa ran over a script-owned plain backdrop, captured at about 140 frames/s.
- **Files:** `artifacts/design-review-1/nusa/` (run 1, 1 s open window, missed the panel) and `nusa-run2/` (3 s window). Analysis: `artifacts/design-tools/analyze.py`.

The live app differs from the website mockup in timing and spring shape:

| What | Live measurement (px = DIP at 96 DPI) |
|---|---|
| Idle on this install | A **44 × 4 sliver** (resting or minimal state). After a close it rests as the 150 × 30 clock pill with ears, with a faint blue-violet glow from the mascot ambience along the bottom |
| Hover intent | About **350 ms** with no reaction |
| Wake | The sliver grows to the 150 × 30 pill over about 500 ms, ease-out, no overshoot |
| Anticipation | The pill swells slowly to about 166 × 36 over about 250 ms before opening |
| Open | A spring from the swollen pill to 420 × 140. It peaks at about **438 × 147 about 230 ms** after the start (≈7 % of travel) and settles by about 460 ms |
| Open size | Body 420 × 140, with ears of about **16 px** (the panel spans 452 px at the top edge) |
| Content | The compact content holds for about 80 ms. Panel content then fades and slides in between about 35 % and 70 % of the size travel, clipped by the growing body |
| Shadow | Darkens the backdrop below the body edge by 0.30, 0.25, 0.20, 0.16, 0.12, 0.09, 0.06, 0.04 and 0.02 at 2–34 px below it (4 px steps). Beside the body at mid-height it darkens 0.30, 0.25, 0.14 and 0.04 at 2, 6, 14 and 24 px (measured by the round-1 Opus judge), so it wraps the sides too. The source is inset only about 6 px |
| Close (run 1) | Starts immediately on leave. Reaches 150 × 30 in about **210 ms**, **undershoots** to about 124 × 22 (≈9 %), and settles by about 480 ms. *The live app bounces on close, unlike the mockup* |
| Close latency | Inconsistent: immediate in run 1, but the panel was still open more than 800 ms after leave in run 2. Not explained |
| Reduced motion | Nusa animated with Windows "Animation effects" **off** on this machine, so it does not honour that setting |

## Dock

Nusa has **no dock** (S1, S3). Its visual language for small controls is white glyphs on black, `rgba(255,255,255,.12)` hover/chip fills and 14 px pill radii (M, S1). Its active-page indicator is a short white pill (M, S2).

## What ROQA will do

ROQA's actual tokens are in `roqa-visual-spec.md`.

- **Same notch silhouette.** Black, flush top, bottom radii, concave ears, drawn as an anti-aliased Direct2D path. That replaces the old aliased GDI window region.
- **Same collapsed pill.** 150 × 30 DIP, r 14, ears 10. ROQA keeps the always-visible clock pill (Nusa's default) rather than this install's 44 × 4 sliver.
- **Hover panel sized to our content.** The user chose a **320 × 80** glance panel (r 24, ears 16), because ROQA has no media yet.
- **Motion fitted to the live capture.** Our own damped-spring maths; the parameters are fitted to the measurements above:
  - **Hover intent:** a 320 ms swell to about 162 × 33.
  - **Open:** a spring with about 7 % overshoot, peaking at about 230 ms.
  - **Close:** a spring with about 9 % undershoot that reaches the pill at about 200 ms.
  - **Content:** crossfades after the body opens.
  - **Shadow:** fitted to the profile above.
  - **Time:** the digits stay large and the am/pm designator is drawn smaller, as in Nusa's header.
- **Frames only while animating.** Frames are paced by the compositor clock, then ROQA returns to the existing fully idle blocking wait.
- **Always animates, like Nusa (user decision, 2026-11-09).** The Windows "Animation effects" setting is not consulted. The earlier reduced-motion snap was removed at the user's request after a capture on this machine showed it snapping.
- **Dock carries Nusa's material.** Solid black, white glyphs, a 0.12-white hover fill, a whole-pixel lift on the hovered icon, and a short white pill for the active app.

## What ROQA deliberately won't copy

- **Nusa's assets.** The mascot/eyes, the green ring, level bars and any Nusa icons, artwork or sounds.
- **Content ROQA doesn't have.** Feature content and pages: media, lyrics, Pomodoro, shelf and so on belong to later milestones, each with its own design, and page dots arrive with them.
- **Nusa's code.** Its CSS/markup, and its bézier constants. Our motion is a damped spring fitted to measured frames.
- **The sliver resting state and the mascot glow.**
- **Nusa's variable close delay.** ROQA closes on leave.
- **Nusa's bundled font.** Nusa uses system Segoe UI Variable, and so do we. No font is bundled.
