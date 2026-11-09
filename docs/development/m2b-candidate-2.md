# M2b candidate 2: candidate 1 + ghost-window fix, packaged/hung/elevated evidence, assisted Narrator session

Author: `/root` (same session as candidate 1). Date: 2026-11-09.
Status: author verification complete; frozen. The user has put the independent reviews **on hold**, and this candidate accepts no milestone by itself. Candidate 1 (manifest `0325C3D4…0BD70A`) was never reviewed. Candidate 2 supersedes it, and the candidate-1 manifest and zip stay preserved for audit.

## Scope

Everything in `m2b-candidate-1.md` still applies (scope, M2b1 finding dispositions, MIN-1 modal-loop decision, self-check SC-1 to SC-7, limits), except where this packet changes it. The changes since candidate 1:

- **Product fix:**
  - **Defect:** Windows' "(Not Responding)" ghost window showed up as a second dock group with a blank icon next to the hung app. The taskbar shows one button for that app. Found by the new packaged/hung check (run 6).
  - **Fix:** `window_source::inspect` now skips top-level windows of class `Ghost`. It reads the class with `GetClassNameW`, which sends no message to the target, so the hung-target safety still holds.
  - **Size:** one guarded early return in `src/window_source.rs`. No new dependency.
- **New evidence script, `scripts/packaged-hung-check.ps1`:**
  - It injects no input.
  - **Setup:** it starts a real hung WinForms window (UI thread in kernel32 `Sleep`, posted `WM_NULL`, `IsHungAppWindow` true) before ROQA's first scan. It then launches Calculator and Settings, two packaged apps whose frames live in one `ApplicationFrameHost` process.
  - **Checks:**
    - separate UIA dock groups;
    - non-blank icon pixels;
    - publication continuing while the hung window is present;
    - a real `Ghost` window existing;
    - exactly one group for the hung app;
    - `--quit` exiting within 5 s with exit code 0.
  - **Cleanup:** it closes only the app instances it started.
- **README:** the stale "no icons or grouping yet" picker text, which candidate 1 still carried, is replaced by a dock and notch section and the new script.

## Finding and self-check updates

- **SC-3 (AUMID lookup on hung targets):** now has runtime evidence. With a real hung window present from startup, the catalog published it and kept publishing packaged windows, and `--quit` took 117–146 ms. One machine, one hang mechanism. This is evidence, not a proof that `SHGetPropertyStoreForWindow` never blocks.
- **SC-5 (silent dock refusal):** now has a concrete case. Windows refuses to minimize an elevated window (below), and the dock gives no feedback.
- Other SC items and the M2b1 dispositions are unchanged.

## M2 acceptance evidence added by this candidate

- **Packaged apps:** AUMID grouping works on real Store apps. Calculator and Settings get separate groups with real icons.
  - Runs: `m2-packaged-hung/run6` on the candidate-1 binary, and `run7` to `run9` plus `m2b-candidate-2/packaged-hung-run1` on this binary.
  - Only these two apps were tried.
- **Elevated windows (user-assisted, `artifacts/m2-assisted/assisted.jsonl`):** Task Manager ran at High integrity (token 0x3000; ROQA and the shell ran at Medium, 0x2000).
  - **Listed and activated:** the dock listed it with its title and icon, and a dock click **activated** it.
  - **Minimize refused:** **Windows refused minimizing it.** The user's physical click had no effect. The same `ShowWindowAsync(SW_MINIMIZE)` call from a Medium shell returned false with error 5 (access denied).
  - **Harness limit:** while a High-integrity window is in the foreground, injected input from Medium is blocked, so that step needed a real click.
- **Narrator (user report, real speech):**
  - **Picker flow passed.** A click on "All windows" opened a focused picker, and Narrator announced the filter edit. Typing, Tab and arrowing read the list items. Enter activated the chosen window.
  - **Dock hover was silent.** The user's Narrator setting for reading under the mouse is off (`InteractionMouse=0`, a read-only registry check). UIA `FromPoint` returns the correct named Buttons for the dock slots.
  - **Not tested:** Narrator reading the dock with that setting on (the user skipped the retest). The dock isn't keyboard-focusable by design; the picker is the keyboard and Narrator route.
- **OS text-scale change: not established.** The attempt didn't apply: Windows reported a factor of 1.0, and the slider was at its minimum. The user skipped the retry. The picker font matched the factor Windows reported.
- **Arabic keyboard or IME typing:** skipped by the user. The Arabic evidence is still UIA ValuePattern input plus rendering.

## Author verification (final binary)

Release `target/release/roqa.exe` SHA-256 `04DA0E517BFBDFA3E674C0C60356A2BDA943CAC7534AC2864F7E4D96898C9F44`; `roqa-uia-probe.exe` `07A86EE58DAB5802C6B3B732B68FEFAEF18494591B4311607E331B68E4AC485D`. The evidence under `artifacts/m2b-candidate-2/` records this binary hash. The user confirmed the desktop was free for every input-injecting run.

- **`verification/`:** `verify.ps1` ran fmt, locked all-target warnings-denied Clippy, **17 tests** and a locked release build. Both steps exit 0.
- **`picker-run1/`:** **PASS 226**. Cycle samples, private bytes / handles / USER / GDI:
  - warm: 19,447,808 / 620 / 35 / 52
  - block 3: 19,824,640 / 624 / 35 / 52

  Handle and USER counts are higher than candidate 1's run (484 / 27). More user windows were open (Settings, more dock groups), and I haven't attributed the difference. These are workload samples, not a leak-freedom proof.
- **`shell-run1/`:** **PASS 304**. Notch hover expands to 360×84 DIP and collapses on leave.
- **`catalog-run1/`:** **PASS 67**. Generation 615, 9 groups, `hooks_live` 0, `stopped` true.
- **`packaged-hung-run1/`:** **PASS 16**. `--quit` took 125 ms.
- **`idle-run1/`:**
  - 30.016 s; CPU **46.875 ms**, which is 3 tick quanta.
  - Private bytes went from 18,538,496 to 18,743,296, and handles from 543 to 531.
  - Higher than candidate 1's 15.625 ms. The desktop was busier, and the dock legitimately repaints or renames on real catalog events from other apps. Not attributed.
  - Observation only: no budget verdict and no competitor comparison.

### Failed and invalid runs (kept)

- **`artifacts/m2-packaged-hung/run1` to `run5`** (candidate-1 binary): FAIL or ERROR, all on harness bugs, fixed in the script only:
  - `$null` became `""` for `FindWindow`;
  - unquoted `Start-Process` arguments;
  - the fixture's `Start-Sleep` CLR wait pumped messages, so the window never hung;
  - `,@()` wrapping hid the frame windows.

  Run 5 already showed the dock and the hung-window group. Run 6 PASSED its 14 checks but revealed the ghost-window **product defect** fixed above.
- **Run 7** (this binary) PASSED before the check required a real `Ghost` window to exist, so on its own it isn't evidence of the fix. **Runs 8 and 9** add that precondition and PASS 16.
- **Assisted session:**
  - The text-size attempt is invalid evidence (the setting never applied). Two captures where the picker was covered by another window were deleted as unusable. A Settings capture was cropped to the slider, and the full image was deleted because it showed the account name and email.
  - The helper's first elevation probe was misleading and was superseded by a token-integrity read (both are recorded in the log).

## Known risks and limits

Candidate 1's limits apply. These changed:

- **Packaged apps:** packaged-app grouping and icons are now verified, for Calculator and Settings only.
- **Hung targets:** a single hang mechanism was covered.
- **Elevated windows:** listing and activation are verified. **Minimize is refused by Windows, without feedback.**
- **Still untested:**
  - Narrator reading the dock;
  - an OS text-scale setting change with live relayout;
  - real keyboard or IME text input;
  - packaged apps beyond the two above;
  - other hang modes.
- Evidence JSON and dock captures contain real window titles and app icons from the author's desktop session (for example browser tab titles). They stay in the local, version-control-ignored `artifacts/`.

## Independent review packet (held)

Manifest `reviews/m2b-iteration-2.manifest`; snapshot `artifacts/review-snapshots/m2b-iteration-2.zip` (paths preserved). The manifest excludes `progress.md`, reviewer output and WIP evidence from other binaries (`artifacts/m2b2-work/`, `artifacts/m2b1-work/`), except `artifacts/m2-packaged-hung/`. That one is included because it holds the evidence for the defect and the fix.

The same reviewer roles as in candidate 1 apply: Sol High offline with clean-code-guard (plus `src/window_source.rs` and `scripts/packaged-hung-check.ps1`), and Opus High holding the exclusive native slot. Reports use the IDs `M2B-SOL-*` and `M2B-OPUS-*`.
