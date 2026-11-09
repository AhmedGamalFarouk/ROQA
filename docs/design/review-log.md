# Design review log (Nusa-like design pass)

Independent judges: Claude Opus 5.5 (effort high), Gemini 3.8 Flash (High) and Sol 6.1 (reasoning medium). The judges are read-only and never author fixes. Every re-judge is a new delegated task. The author is `/root`. Evidence lives under ignored `artifacts/design-review-N/`.

## Measurements behind the tuning

- **Shadow.** Live Nusa's shadow darkens the backdrop below the open body edge by 0.30, 0.25, 0.20, 0.16, 0.12, 0.09, 0.06, 0.04 and 0.02 at 2–34 px, in 4 px steps (`artifacts/design-review-1/analysis.json`). That profile was fitted with a body source inset 16 DIP, ending 7 DIP above the bottom, σ 20 DIP and alpha 0.9.
- **Open spring.** It peaks at about 7 % of travel about 230 ms after the start. Fitted with ζ 0.65 and ω 18.
- **Close spring.** It reaches the pill at about 210 ms and undershoots about 9 %. Fitted with ζ 0.61 and ω 16.
- **Swell.** About +12 × +5 over about 250 ms. Fitted as progress 0.07 over 320 ms.

## Round 1

- **Inputs.** Manifest `artifacts/design-review-1/inputs.sha256`, SHA-256 `A69E033E80C9675B5DDCE48A63DF20DF7C586FCD2B82C2DC38F6B09281F33BFE`.
- **Binary.** `5F70A5848ED9EF69B39A669EDEE311AC12AF46E7B794F41E29424315FFCCF6C0`.
- **Brief.** `artifacts/design-review-1/judge-brief.md`.

| Judge | Task | Verdict |
|---|---|---|
| Opus 5.5 High | [thread](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3A680a425a-dc0a-48ee-ac8d-7418cdb944d1>) | **REVISE** (manifest match YES) |
| Gemini 3.8 Flash High | [thread](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3A6be782d1-b99f-4313-9e1d-3b7fba6602b5>) | **POLISHED** (manifest match YES; 3 minors) |
| Sol 6.1 Medium | [thread](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3A3f4aebc0-4028-4e24-919a-320bfbabdf2c>) | **REVISE** (manifest match YES) |

### Sol round-1 findings

| ID | Severity | Summary | Disposition |
|---|---|---|---|
| DESIGN-SOL-1 | major | Input over the whole 368×124 window during swell, open and close; transparent sides and shadow band intercept clicks; hover checks only Y | open |
| DESIGN-SOL-2 | major | Notch text ignores the Windows text-scale factor (pre-existing documented limit) | open |
| DESIGN-SOL-3 | minor | Crossfade gap: compact text is gone at p = 0.30 and the panel starts at 0.35, a blink of ~26–29 ms | open |
| DESIGN-SOL-4 | minor | Per-frame creation of formats, layouts, effects and bitmaps; unmeasured cost | open |

### Opus round-1 findings

| ID | Severity | Summary | Disposition |
|---|---|---|---|
| DESIGN-OPUS-1 | major | Same root issue as SOL-1. The region opens to the whole window at the first pill touch, so the side margins hold or re-engage the hover and clicks are swallowed. Hover tests only Y | open |
| DESIGN-OPUS-2 | major | The pill clock is about 20 % smaller and lighter than Nusa's: cap 9 vs 11 px, semibold vs bold, designator 0.62 vs ~0.72 | open |
| DESIGN-OPUS-3 | minor | Crossfade blink, as SOL-3; suggests one scaled clock element | open |
| DESIGN-OPUS-4 | minor | Side shadow is about ⅓ of Nusa's. Inset 16 → 6 DIP, and widen the window so the shadow isn't clipped | open |
| DESIGN-OPUS-5 | minor | Dock highlight stays put while the icon lifts; use a 48×48 chip centred on the lifted icon | open |
| DESIGN-OPUS-6 | minor | Per-frame resource churn, as SOL-4 | open |
| DESIGN-OPUS-7 | minor | `DCompositionWaitForCompositorClock` is statically imported (Win11 22000+), so the exe would fail to load on Windows 10 | open |
| DESIGN-OPUS-8 | minor | Date contrast is 7.4:1, not 8.6:1; the shadow comment points to the wrong doc | open |

### Gemini round-1 findings

| ID | Severity | Summary |
|---|---|---|
| DESIGN-GEMINI-1 | minor | Per-frame resource churn (as SOL-4 and OPUS-6) |
| DESIGN-GEMINI-2 | minor | Clicks in the shadow band are swallowed while open (as SOL-1 and OPUS-1) |
| DESIGN-GEMINI-3 | minor | The dock's transparent anti-aliased corners still take input |

### Round-1 dispositions (author `/root`; only a fresh judge can mark a fix resolved)

- **SOL-1, OPUS-1, GEMINI-2: fix-submitted.**
  - **Region.** `layout::notch_region` now sets the window region every frame. It is the current body and ears plus a shadow reach of 44 DIP × √smooth(p), with a two-frame lead while growing. It is the exact 170×30 footprint at rest, barely larger than the pill during the hover intent, and the whole window only when the shadow is fully drawn.
  - **Hover.** `layout::notch_hit` tests x and y against the drawn body plus 4 DIP, so the margins and shadow never hold or re-open the panel.
  - **Shared geometry.** Drawing, hover and region share `layout::notch_body`.
  - **Smoke.** It now asserts the open region box, and adds a "pass over the pill sideways within 320 ms never opens" check.
- **SOL-2: fix-submitted.** The notch's effective DPI is monitor DPI × `UISettings.TextScaleFactor`, and the notch is rebuilt live on `TextScaleFactorChanged` through the existing `TextSettings`. It is verified only at factor 1.0: a non-1.0 factor would need the user to change a setting.
- **OPUS-2: fix-submitted.** The pill clock is 15 DIP, bold and white, with the designator at 0.72×; the panel time is bold too.
- **SOL-3, OPUS-3: fix-submitted.** One clock element morphs: 15 → 30 DIP, centre 15 → 29, with no fade, and only the date fades. Frames are in `design-review-2/evidence/13-roqa-morph-frames.png`.
- **OPUS-4: fix-submitted.** The shadow source side inset is now 6 DIP (was 16), and the window is 400 wide so the side shadow isn't clipped.
- **OPUS-5: fix-submitted.** The dock highlight is a 48×48 chip centred on the lifted icon.
- **OPUS-7: fix-submitted.** `DCompositionWaitForCompositorClock` is resolved with `GetProcAddress`; if it is absent, the loop uses a ≤ 16 ms message wait.
- **OPUS-8: fixed in docs.** The date contrast is now stated as 7.4 : 1, and the shadow comment points to `nusa-reference.md`.
- **SOL-4, OPUS-6, GEMINI-1: deferred (minor).**
  - Per-frame object creation is bounded to user-triggered motion, at most about 0.9 s per interaction, and returns to idle.
  - No cost has been measured, and caching D2D effects across DirectComposition `BeginDraw` contexts needs separate verification.
  - Revisit with a frame-time trace; the `shortcut:` comment on icon bitmaps stays.
- **GEMINI-3: deferred (minor).** The transparent corner pixels only take input on the top and bottom one or two pixel rows at x 8–20 DIP. A GDI region there would alias the anti-aliased corners.

## Round 2

- **Inputs.** Manifest `artifacts/design-review-2/inputs.sha256`.
- **Binary.** `63CE48159B82DCF63A7C21D198813047FC7D3CFF1FCBCBD78FE458B7BFA52059`.
- **Brief.** `artifacts/design-review-2/judge-brief.md`.
- **Manifest SHA-256.** `334FB2DACB49B2E7F8349B69F3B3E844471012C92464C9CECDBFF1DFA55A3BE1`.

| Judge | Task | Verdict |
|---|---|---|
| Opus 5.5 High | [thread](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3A2b56b372-7cfa-4208-94c3-7a3664a3fc46>) | pending |
| Gemini 3.8 Flash High | [thread](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3Ade75aab9-9e7c-478f-a8c6-ea107a4634f4>) | pending |
| Sol 6.1 Medium | [thread](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3A9da5142e-e27f-4ef8-b0f6-7810f042c9c1>) | pending |
