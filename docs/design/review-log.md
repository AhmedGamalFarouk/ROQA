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
| Opus 5.5 High | [thread](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3A2b56b372-7cfa-4208-94c3-7a3664a3fc46>) | rate limited; deferred by user; cancellation requested |
| Gemini 3.8 Flash High | [thread](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3Ade75aab9-9e7c-478f-a8c6-ea107a4634f4>) | POLISHED; manifest match YES; report review-round-2-gemini.md |
| Sol 6.1 Medium | [thread](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3A9da5142e-e27f-4ef8-b0f6-7810f042c9c1>) | REVISE; manifest match YES; report review-round-2-sol.md |

## Round 3 preparation (author /root, Codex continuation)

Claude judgment is deferred by the user's instruction to skip it for now. Its failed task was cancelled; no replacement launched. This does not accept M2 or release the held candidate reviews.

Sol's remaining major is shadow input interception (SOL-1 / OPUS-1 / GEMINI-2). Fix-submitted: the shadow now has a separate layered, transparent, nonactivating HWND, owned alongside the notch and destroyed with it. Only the body-and-ears bounds remain in the body's window region. Hover geometry and visual tokens are unchanged. Fresh independent verification is still required, including actual underlying-process click delivery during closing; WindowFromPoint alone is partial evidence.

Author checks: verification PASS, 21 tests, binary `045273D09564544BD917647B3243F13135DE7BE838CB6896E3222CE4ADEC100B`. Corrected shell run `artifacts/design-review-3/shell-run2` PASS, including open/collapsed shadow-band WindowFromPoint assertions and resource/lifecycle checks. `shell-run1` is an invalid harness invocation: a BOM encoding error bypassed parameter parsing; runtime assertions passed but outputs went to the project root. Outputs were moved unchanged into the run folder with a warning; BOM repaired before run2. Each run had fresh desktop permission and T3 Code foreground preflight.

Independent Sol High clean-code-guard review launched on `artifacts/design-review-3/inputs.sha256`; source/binary stay fixed during review. Full native suite, before/after idle, fresh visual capture, and fresh Sol/Gemini design judgments remain pending. No freeze or acceptance.


## Independent code review 3 and silhouette fix (review pending)

- Reviewer: fresh Sol High clean-code-guard task [report thread](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3A58e4679e-c89b-4a1c-9e6b-dc44ea2ef3a2>), report `code-review-3.md`. Verdict REVISE on manifest `BA21D0119728A508C189E7094111818C92339E085DAC1C887C75E8FD4045FBB1` (all 29 entries matched).
- CODE-SOL-001 major: the first separate-shadow fix retained a rectangular ear strip, so (30,40) at 96 DPI still intercepted clicks and held hover. Author /root fix-submitted: analytic silhouette rows for the GDI region, circular-edge hover with four-DIP slop, shared body/radius clamp with D2D, and identical frame progress for region and draw with no growth lead.
- CODE-SOL-002 minor: occlusion/unexpected compositor-clock statuses skipped fallback. Fix-submitted: accept only event, extra clock index and timeout; otherwise bounded message wait. Microsoft's compositor-clock example defines the extra tick index; its API reference documents immediate display-off occlusion. No display-off experiment was run or system configuration changed.
- Author verification `artifacts/design-review-4/verification`: PASS, 22 tests, release build. Binary `21BBFE1A70CB87AAFE48268AC75469B6335BD566FEEE2A51851703BB8ED428C6`.
- Fresh native runs, each with separate confirmation and T3 Code foreground preflight: shell PASS 325, picker/dock PASS 226, catalog PASS 67, packaged/hung PASS 16 (quit 144 ms). Shell evidence includes actual underlying-process clicks in rest corner, opening side shadow, open side shadow/corner, and closing side shadow/corner. These tests caught a gap in the earlier lower-band-only assertions and do not replace independent approval.
- Fresh visual capture: `artifacts/design-review-4/roqa`, active fixture foreground true, graceful quit 0. Side-by-side evidence `evidence/01`?`04`; GIFs/curves `05`?`11`; dock states `12`; morph frame sheet `13`. Body 320x80; shadow profile preserved. Pixel-region edge quality still awaits independent judges. Current evidence and older failed/invalid runs remain on disk.
- Always-animate user decision and existing M2 manual limits stay unchanged. Claude judging deferred; no candidate-3 milestone freeze, council approval or commit.

## Fresh design and code rechecks on the silhouette fix

- Exact inputs `artifacts/design-review-4/inputs.sha256`, 609 entries, SHA-256 `15B6A13CB06C16E37D7112C02751DAA57A594CF5FEA00A491CFE61CE0FA4F67F`.
- Path-preserving review-input archive `artifacts/design-review-4/inputs.zip`, SHA-256 `6C22AE9FC9E19F5EF2EBF47A8B244BDCE43F23E8DCF872115EC10E39FB5CCA00`. This preserves WIP review inputs; it does not freeze M2 candidate 3.
- All required author native scripts passed on binary `21BBFE1A…28C6`: shell 325, picker/dock 226, catalog 67, packaged/hung 16. Separate baseline/current idle samples completed, each user-confirmed and foreground-preflighted; raw observations and limits in the artifact README. No idle-budget or animation-cost verdict inferred.
- Design recheck: Sol Medium, silhouette fix: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3A37bf26f6-6314-480f-a912-e8d825b5f214> (pending).
- Design recheck: Gemini High, silhouette fix: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3Aab09d185-064d-4241-b2f8-b13c9bc06ad4> (pending).
- Clean-code-guard recheck: silhouette and clock statuses: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3A1acf0a00-5732-4474-ba38-b0fb98044eac> (pending).
- No self-approval or stale verdict. Implementation and reviewed evidence stay fixed until the fresh tasks finish. Source-of-truth acceptance state remains open.

## Round 3 Gemini completion (2026-11-09)

Gemini 3.8 Flash High returned **POLISHED**, independently verifying all 609 entries of manifest `15B6A13CB06C16E37D7112C02751DAA57A594CF5FEA00A491CFE61CE0FA4F67F`. No new scoped findings. It resolved the silhouette/input and compositor-status findings and accepted the existing minor allocation/dock-corner deferrals. Disposition record: `docs/design/review-round-3-gemini.md`; full report: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3Aab09d185-064d-4241-b2f8-b13c9bc06ad4>.

Sol Medium design and Sol High clean-code-guard results remain pending. Claude remains deferred. This is a WIP review, not candidate-3 milestone acceptance. No source/evidence input was changed for this record; candidate freeze and council remain held. Hardware text scale, Windows 10 execution, physical Narrator/Arabic/IME and compositor/performance limits remain open. A 0-ms counter sample is not proof of zero cost.
## Round 3 remaining results and expansion-probe correction (2026-11-09)

Sol Medium design returned REVISE on the exact 609-entry manifest 15B6A13C...4F67F: DESIGN-SOL-R3-001 major validation gap. The opening-labeled click at 120 ms occurs during 320 ms hover intent; expansion pass-through was not established. No visual redesign or new production failure demonstrated. Report record: docs/design/review-round-3-sol.md; full source thread linked there.

Independent Sol High clean-code-guard returned ACCEPT code-only on the same inputs, 0 findings, CODE-SOL-001/002 resolved. Report record: docs/design/code-review-4.md; full source thread linked there. This does not close the runtime evidence gap or grant milestone acceptance.

Author /root corrected scripts/smoke.ps1 only: retained and accurately renamed hover-intent case; added a geometry-synchronized expanding-side-shadow case at observed region height 48–64 DIP, with real fixture click and post-injection height below 76 DIP and not shrinking. Records region bounds before/after, local click coordinates, observation timing and fixture receipt. Production binary unchanged. Script parse passes; corrected native run is pending fresh desktop-free confirmation. Old review-4 manifest/archive/evidence remain preserved; its script disk entry is now superseded. Fresh scoped recheck required after new evidence. Claude and candidate-3 freeze/council remain held; nothing committed or accepted.
## Expansion-phase evidence submitted (2026-11-09)

Fresh user confirmation and foreground preflight (T3 Code Nightly PID20204) preceded corrected shell-run1 in artifacts/design-review-5. PASS329, all seven real fixture clicks received. Expansion observed region82,0,236,48 ->72,0,256,54, click80,24; 374ms to observed expansion and 8ms observation/injection bracket. Intent remained33px high; open80px; closing70->67 and67->64. No production code/binary changed. Against prior manifest, only scripts/smoke.ps1 changed; old archive/evidence preserved. Full suite/visual/idle evidence remains on identical production inputs; no unnecessary GUI reruns.

Fresh WIP exact manifest: artifacts/design-review-5/inputs.sha256, 622 entries, SHA256 905E1E912620A709FDBFB89D632ADF4FB1AB32F6879C6E0A5074D7806265493C. Path-preserving ZIP SHA256 2683A0ADC6616F0167A207D2EB786DD50991E9734BF257657DE9791E7C587E69. README/brief explain the superseded opening evidence and scope.

Fresh scoped Sol Medium review: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3A3501030e-7f16-431e-9969-45d2b301aefe> (pending).
Fresh scoped Gemini High review: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3A40697107-7c73-40f5-9b75-2aa6371f579d> (pending).

Independent code-only ACCEPT retained for unchanged production code; expansion finding remains fix-submitted pending independent recheck. Claude deferred, M2 council/candidate-3 freeze held, no acceptance/commit. Manual and performance limits unchanged.
## User acceptance and commit authorization (2026-11-09)

User explicitly instructed: "accept and commit" after the corrected PASS329 shell run and launch of the scoped evidence rechecks. Record this as user acceptance of the current design pass and authorization to commit its implementation, harness and review records. It is not an independent reviewer verdict: latest Sol/Gemini expansion-evidence rechecks are still pending at commit preparation. Claude remains deferred. Independent clean-code-guard ACCEPT applies to unchanged production code.

All 622 review-5 inputs reverified against the manifest immediately before commit preparation; git diff --check passed. No additional GUI run or production edit. Existing full-suite results and hardware/accessibility/performance limits remain as documented. M2 candidate-3 freeze and milestone council are not performed or accepted by this design commit. Unrelated user .gitignore edit is excluded. Ignored binaries, captures and review archives remain local.