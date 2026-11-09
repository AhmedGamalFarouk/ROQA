# M0 iteration 2 — independent Sol recheck

- Reviewer: GPT-6.1 Sol High, fresh review task, 2026-11-09. This task did not author the candidate or its fixes and wrote only this report. No nested agents were used.
- Candidate author: primary implementation agent `/root`, as recorded in `PLAN.md:3` and the development ledger. Sharing the model does not share task authorship.
- Scope: recheck round 1 of 3; prior findings, revised plan changes against the old scope described in the initial reports, and resulting regressions. Both iteration-1 reports were read for finding context. The iteration-2 Opus report was not read.
- Disposition: **ACCEPT**, exclusively for manifest SHA-256 **`30E9BDE5B31564D89EDE3B393F074C19EA008B2681A9073DAE2C33C63D962133`**. No open blocker or major finding.

## Input integrity

Read `AGENTS.md`, `PLAN.md`, `PRODUCT.md`, `docs/development/progress.md`, `docs/council/decision.md`, and `docs/council/assessment-notes.md`. The requested assessment notes are under `docs/council/`; no root-level file exists.

Recomputed the manifest hash and every entry using PowerShell `Get-FileHash -Algorithm SHA256`. The manifest is `docs/development/reviews/m0-iteration-2.manifest`; its hash matches the supplied value above, and all five entries match:

| Frozen input | Verified SHA-256 |
| --- | --- |
| `PLAN.md` | `A23267C5F8C9A83BCFCE1EF5C600C260932E6F05C0D38BB4E2861A76F98D2DB0` |
| `AGENTS.md` | `269BEF4A647ED57C98F770526820761B504F4C14C035BD3A1C9B4747EB19F40C` |
| `PRODUCT.md` | `15CB07B3A2145E936E2B8419BFE96AB01C4CC2BA2913B23D61356A1E132F758A` |
| `docs/council/decision.md` | `8AC659F51ADACFE69D746B1F6A88E1E52177AE223A6704C3BF75CEF141BCF265` |
| `docs/council/assessment-notes.md` | `757A63E68F14587A2E04B13949706EB0181819C8538121227FB94437410E6445` |

`AGENTS.md`, `PRODUCT.md` and both council context files also match the hashes recorded in the initial reports. The old plan's scope was reconstructed from those reports; no archived old-plan text was assumed available. `progress.md` is intentionally mutable and excluded under `PLAN.md:101`; its observed hash was `8B1122CF9F35391A8D3B5C1E07CF408504DBC5A8B6C39B2C8FDFAB0E981419F8`. It supplies administrative context, not additional frozen acceptance conditions.

## Prior material findings

The initial Opus report's MAJ-1 through MAJ-4 are carried here under the ledger's stable prefixed IDs. All are **resolved** as documentation requirements, not as completed implementation tests.

| Stable ID | Status | Evidence and assessment |
| --- | --- | --- |
| M0-OPUS-MAJ-1 | Resolved | `PLAN.md:54–66` bounds graphics exploration to two implementation candidates, then requires a reviewed pivot/rescope. It defines reversible pointer-enter/leave composition feedback; nonactivating windows; a per-user/session `roqa --quit` path; foreground, geometry, bounded-window, independent close/reopen and last-surface-exit checks; exit code 0 and forced-kill failure. Ten warm-up plus 100 in-process create/destroy cycles record private bytes, kernel handles, USER/GDI counts and cooldown, with explicit tolerances and partial-initialization failure cleanup. DPI and callback failure evidence are required. Keyboard focus entry remains correctly scoped to M2. |
| M0-OPUS-MAJ-2 | Resolved | `PLAN.md:64` requires actual screen-region captures of both visible windows, solid-content pixel assertions and the interaction transition, with images/results retained. HWND existence and offscreen-only rendering cannot pass. Capture failure leaves M1 open pending a scope waiver or reviewed pivot; this review grants neither. |
| M0-OPUS-MAJ-3 | Resolved | `PLAN.md:73` requires actual Arabic/RTL, system-text-scale and IME-route evidence before custom-control expansion, followed by a council-reviewed stack go/no-go before M3. It references M1 renderer evidence and council pivot criteria. `PLAN.md:77–85` schedules mixed-DPI/lifecycle coverage, lock/unlock, injected device loss/recreation, hung-source isolation and reduced-motion behavior. These risks are no longer silently deferred to M5. |
| M0-OPUS-MAJ-4 | Resolved | `PLAN.md:24–41,47` requires independent fresh tasks, stable finding IDs/statuses, bounded recheck scope, nonblocking minors, evidence/recheck for disputed material findings, user escalation for unresolved conflict, a three-round cap and both ACCEPTs on the identical final manifest hash. Accepting the stricter recommendation is explicitly permitted; the author has done so rather than waived or adjudicated a material objection. |

## Prior minor findings and suggestions

| Stable ID | Status | Evidence |
| --- | --- | --- |
| M0-OPUS-MIN-1 | Resolved | Both normative council files are now hashed; `PLAN.md:101` explicitly excludes the mutable ledger. |
| M0-OPUS-MIN-2 | Resolved | `PLAN.md:14,62` specifies official checksummed rustup, verified flags, no PATH modification, local homes/wrappers, exact toolchain/components/target, ignore rules before bootstrap, locked commands and reuse of installed MSVC/SDK. |
| M0-OPUS-MIN-3 | Resolved | `PLAN.md:54,63` specifies work-area placement clear of the taskbar, nonactivating tool windows, no AppBar/work-area reservation and geometry/work-area checks. This resolves the placement and shell-state concern; topmost behavior is not an additional M0 acceptance gate. |
| M0-OPUS-MIN-4 | Resolved | `PLAN.md:55` makes a WinUI pivot a reviewed amendment with a pinned local .NET SDK bootstrap. |
| M0-OPUS-MIN-5 | Resolved | `PLAN.md:103` requires an identified independent clean-code-guard pass in M1+ packets. |
| M0-OPUS-MIN-6 | Resolved | `PLAN.md:67` specifies release build, five-second settle, 30-second sampling, tool/counters, hardware/OS fields, limitations/variance and an observation-only label. |

The initial Sol suggestions had no stable IDs; retain their original numbering: suggestion 1 (renderer bound), 2 (early text/input probe) and 3 (finite meaningful lifecycle evidence) are all **resolved** by the corresponding requirements above. The initial Opus suggestions on callback panic strategy/testing, COM apartment/teardown ownership and embedded per-monitor-v2 DPI awareness/`WM_DPICHANGED` are explicitly required at `PLAN.md:66`. Independence remains explicit at `PLAN.md:41`.

## Regressions, limits and final disposition

No new material regression was found in the revised plan. No prior finding remains partial or open. The added checks make future acceptance concrete without claiming that they have run. The temporary control/IPC host permitted for in-process churn is compatible with testing normal last-surface shutdown separately.

This was a documentation and integrity review only. No toolchain installation, build, runtime capture, accessibility test, benchmark, implementation edit or system change was performed. Production code-style review does not apply to M0 prose. Future runtime evidence remains mandatory at its specified milestone and is not a prerequisite for accepting M0's plan.

**ACCEPT M0 iteration 2 for manifest SHA-256 `30E9BDE5B31564D89EDE3B393F074C19EA008B2681A9073DAE2C33C63D962133`.** This is the Sol council disposition only. Once the independent Opus reviewer also accepts this same manifest and the ledger records the gate, proceed to the authorized project-local toolchain and M1 native shell implementation. No further user decision is required for the submitted plan corrections.
