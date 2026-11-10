# Remaining M2 assisted validation

Prepared 2026-11-09 by primary Codex agent (gpt-6.1-sol, medium). NOT RUN; no acceptance or independent review. Council deferred by user, no replacement judges.

## Baseline and evidence

HEAD: ae6693645bf0434898d91784a2d41323e8246366. Existing target/release/roqa.exe SHA-256 verified offline: 21BBFE1A70CB87AAFE48268AC75469B6335BD566FEEE2A51851703BB8ED428C6.

Inspected picker, font, text-settings and host source, picker smoke, native UIA probe and run wrapper. Ledger is evidence authority. Latest picker/dock PASS 226 proves automated keyboard/UIA cases, not current physical speech/input. Prior user-assisted Narrator flow used the earlier 04DA0E51... binary. Arabic rendering/UIA injection is not physical typing; Arabic keyboard input is not IME composition. Earlier real text factor was 1.0; subscriptions and simulated DPI do not prove live text-size changes.

## Run safety

Before EACH native run obtain fresh explicit desktop-free confirmation and check foreground owner immediately before launch/input. Stop on interference. Prior permissions are spent. Prefer user-driven cases over broad input-injecting suites. Confirm no pre-existing ROQA, retain launched PID and stop only that instance. Use recorded binary; scripts/run.ps1 start rebuilds and would invalidate unchanged-binary claims.

Desktop-free consent does not authorize settings changes, elevation or layout/IME installation. User operates existing Narrator/input; obtain separate authorization for text-size changes. Preserve taskbar and always-animate choice. Crop captures to ROQA, avoid private titles/account details, label speech as user report unless recorded. New raw evidence: artifacts/m2-assisted-next/<session>/; preserve old evidence.

## A. Keyboard and Narrator

1. Start passively with a non-sensitive target; verify foreground preservation. From foreground terminal invoke scripts/run.ps1 show-picker; record focus and spoken control.
2. Physically filter, Tab, arrow and Enter; verify actual target foreground. Reopen; test Tab/Shift+Tab through filter/list/Open/Close, and Escape focus return.
3. Enter no-match text: disabled Open, zero-result status, Narrator discovery through navigation. Automatic announcement is a separate observation. Clear and verify recovery.
4. If safe, close selected disposable target while picker is open; record row removal or recoverable activation message. Row removal does not prove the stale-selection branch ran.
5. Record foreground-denial explanation/recovery only if genuinely observed. Using existing Narrator navigation, try dock toolbar/All windows reading and invocation. If it needs preference changes, NOT RUN. Silent hover with mouse interaction off is not a provider failure or speech pass.

## B. Physical Arabic/RTL

Needs an existing Arabic layout and a real mixed Arabic/Latin/digit window title. Document content alone is not a matching title. Physically type a matching Arabic substring; no paste/UIA/synthetic Unicode. Record text/layout/title, joining, direction, punctuation/digits, arrows/Home/End, selection/Backspace. Verify filtering and real activation. Restore original layout. Readable bidirectional text does not claim mirrored/localized UI.

## C. Physical IME

Needs an installed composition-capable IME and user able to operate it; otherwise NOT RUN, no installation or proxy substitution. Show preedit/candidates, select/commit, verify value/caret and no premature app activation. Start/cancel composition; verify no unexpected picker dismissal. Test ordinary Enter/Escape after composition ends. Verify matching-title activation if available, otherwise record that subcase NOT RUN. Restore original input method.

## D. Live text size (separate authorization)

Record original setting, fresh UISettings.TextScaleFactor, PID, monitor DPI, picker font/rectangle and cropped baseline. User applies non-1.0 text size (e.g. 125%) with same ROQA process/picker open; no registry writes or synthetic messages. Read fresh factor/font/geometry using existing helpers or a bounded read-only probe; validate/hash any new probe before run. Inspect picker/notch clipping, status/buttons and keyboard usability. Test activation/reopening. Restore exact original setting with user and verify same-process recovery. Slider/registry values alone are insufficient. Report restoration failures explicitly.

## Results and M3 boundary

Per case record time, operator/author, binary hash/PID, OS/display/DPI, relevant Narrator/layout/IME state, actual steps, observations, evidence paths and PASS/FAIL/INCONCLUSIVE/NOT RUN. Separate user reports, instrumented values and visual observations. Stop on interference, elevation, private capture or focus traps; report cleanup/restoration failures.

No production edit, freeze or acceptance here. Any fix needs targeted revalidation and fresh independent non-author clean-code-guard even while council is deferred. Do not rerun finished suites without cause.

First M3 slice should be shared monitor/topology state and deterministic placement before previews: monitor/work-area/DPI identities, negative coordinates/removal, shared catalog and surface lifetime reconciliation. Hardware topology/fullscreen/cleanup evidence is still required; previews, DWM/GPU costs, display-off and device loss stay unproven. This is sequencing, not API selection or permission to change displays. Complete required M2 evidence, or obtain an explicit scope decision for unavailable prerequisites, before bypassing that gate. Council deferral alone is not M2 completion. PLAN.md is unchanged.
