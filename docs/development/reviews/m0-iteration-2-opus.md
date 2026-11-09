# M0 iteration 2 — independent recheck, round 1 of 3 (Opus 5.5 High)

- Reviewer: Claude Opus 5.5 High. This is a fresh delegated recheck task. It is read-only except for this report. The reviewer authored none of the reviewed files and none of the fixes.
- Candidate author: primary agent `/root` (GPT-6.1 Sol High), per `PLAN.md:3` and `docs/development/progress.md` § iteration 2.
- Date: 2026-11-09.
- Finding context came from `m0-iteration-1-opus.md` and `m0-iteration-1-sol.md`. The other iteration-2 reviewer report was not read; no such file existed in `docs/development/reviews/` during this review.
- Scope: prior findings, the revised `PLAN.md` diff (compared with the old scope described in the iteration-1 reports) and any regressions caused by the edits. New findings on unchanged content were admitted only at blocker/major severity with concrete evidence. None qualified.

## Frozen inputs

Manifest `docs/development/reviews/m0-iteration-2.manifest`, SHA-256 `30E9BDE5B31564D89EDE3B393F074C19EA008B2681A9073DAE2C33C63D962133` (recomputed: match).

| File | Manifest SHA-256 | Recomputed | Match |
| --- | --- | --- | --- |
| `PLAN.md` | `A23267C5…F98D2DB0` | `a23267c5…f98d2db0` | yes |
| `AGENTS.md` | `269BEF4A…EB19F40C` | `269bef4a…eb19f40c` | yes (unchanged from iteration 1) |
| `PRODUCT.md` | `15CB07B3…132F758A` | `15cb07b3…132f758a` | yes (unchanged from iteration 1) |
| `docs/council/decision.md` | `8AC659F5…41BCF265` | `8ac659f5…41bcf265` | yes (same as context hash in iteration 1) |
| `docs/council/assessment-notes.md` | `757A63E6…410E6445` | `757a63e6…410e6445` | yes (same as context hash in iteration 1) |

`docs/development/progress.md` is excluded by design (`PLAN.md:101`). It was read as context at `8b1122cf…981419f8`. This disposition does not depend on it.

## Prior findings

| ID | Status | Evidence |
| --- | --- | --- |
| M0-OPUS-MAJ-1 | **Resolved** | Timebox: two implementation candidates, then a pivot/rescope candidate for council review (`PLAN.md:55`). Interactive state: pointer-enter colour/opacity change that reverts on pointer-leave, without activation (`:55`). Checks: foreground HWND unchanged; both rects checked against work-area/DPI geometry; bounded HWND sizes; no AppBar/work-area change; independent close/reopen; last-surface shutdown (`:54`, `:63`). Exit: `roqa --quit` through per-user/session IPC works without focus; it requires exit code 0 and counts a forced kill as a failure (`:56`, `:63`). In-process cycles: 10 warm-up + 100 measured cycles with private bytes, handles and USER/GDI counts, plateau tolerances and partial-init failure injection (`:65`). |
| M0-OPUS-MAJ-2 | **Resolved** | `PLAN.md:64` makes actual screen-region capture of both windows mandatory. It requires pixel assertions for the solid content and for the enter/leave transition. An HWND or an offscreen test alone is insufficient. If capture is impossible, the gate stays OPEN and M1 cannot be accepted without a user waiver or a reviewed pivot. |
| M0-OPUS-MAJ-3 | **Resolved** | M2 now proves one Arabic/RTL title, system text-scale changes and one IME-capable edit route before custom controls expand. It requires recorded evidence (`PLAN.md:73`). A council-reviewed stack go/no-go ends M2, and M3 cannot start before it (`:73`). Lock/unlock, device loss and hung source app are in M3 (`:79`). App disappearance/hang and reduced motion are in M4 (`:85`). The list matches `decision.md:64–66, 84, 100`. |
| M0-OPUS-MAJ-4 | **Resolved** | `PLAN.md:33–39` adds stable IDs with statuses, and only an independent review can resolve a fix. Minors never block. Recheck scope is bounded, and new findings on unchanged content need blocker/major evidence. A dispute gets a fresh recheck and then goes to the user; the author cannot waive it. The loop is capped at three rounds and then escalates. Acceptance needs both ACCEPTs on the identical final manifest SHA-256, and earlier accepts cannot be combined. |
| M0-OPUS-MIN-1 | Resolved | Normative council docs are hashed, and the ledger is excluded with a stated reason (`PLAN.md:101`; manifest). |
| M0-OPUS-MIN-2 | Resolved | `PLAN.md:14`: `.gitignore` before bootstrap, checksummed official `rustup-init`, verified flags, `--no-modify-path`, local homes set only in wrappers, exact pinned toolchain/components/target, `--locked`, reuse of MSVC 14.44 / SDK 10.0.26100.0. |
| M0-OPUS-MIN-3 | Resolved | Windows sit inside the work area with no AppBar or work-area reservation, as nonactivating tool windows (`PLAN.md:54`). |
| M0-OPUS-MIN-4 | Resolved | A WinUI pivot is a reviewed amendment with its own pinned local .NET SDK bootstrap (`PLAN.md:55`). |
| M0-OPUS-MIN-5 | Resolved | M1+ packets require an independent clean-code-guard pass by a non-author council reviewer (`PLAN.md:103`). |
| M0-OPUS-MIN-6 | Resolved | Fixed method: release build, 5 s settle, 30 s sample, named counters/tool, OS/hardware recorded, overhead/variance noted, labelled observation only (`PLAN.md:67`). |

Iteration-1 suggestions are covered. The panic strategy, COM apartment and composition teardown order are recorded and exercised (`PLAN.md:57`, `:66`). Per-monitor-v2 DPI awareness is embedded and `WM_DPICHANGED` is exercised (`:66`). Sol's three suggestions (renderer timebox, early text/RTL/scale probe, finite lifecycle method with last-surface and partial-init behaviour) are covered at `:55`, `:73`, `:54` and `:65`.

## Regressions from the edits

No material regression. The edits stay inside `AGENTS.md` constraints. Test-time pointer synthesis and screen capture need no hooks, elevation or global settings changes. Session-scoped IPC is user-scoped. The no-AppBar rule protects the taskbar and work area.

Two non-blocking clarifications for the M1 candidate packet (minor; no M0 edit required):

- **M0-OPUS-R2-MIN-1.** `PLAN.md:54` says closing the last surface exits the host. `:65` cycles window/composition objects "retaining a control/IPC host as needed". The M1 packet should name the mechanism, either cycling one surface while the other stays alive or a test-only resident mode. That keeps the lifecycle test from contradicting the shipped last-surface behaviour.
- **M0-OPUS-R2-MIN-2.** The plateau tolerances at `:65` should be stated as pass/fail limits in the M1 packet. Any monotonic growth across blocks is a failure even when it stays under tolerance. Today the wording ("investigate … initial plateau tolerances") reads as intended but not explicit.

## Evidence limits

This is a documentation-only gate. Nothing was built, run, installed or measured. The runtime requirements above are obligations for M1–M4, not claims that they have been met. The next step (local toolchain bootstrap and M1 native shell) is not blocked on future runtime evidence.

## Disposition

**ACCEPT M0**, scoped exactly to manifest `docs/development/reviews/m0-iteration-2.manifest` at SHA-256 `30E9BDE5B31564D89EDE3B393F074C19EA008B2681A9073DAE2C33C63D962133` and the five file hashes above. No blocker or major finding remains open. M0-OPUS-MAJ-1 to MAJ-4 are resolved. Any change to a manifest-covered file invalidates this acceptance. M0 is accepted only when the independent Sol recheck also ACCEPTs this identical manifest hash.
