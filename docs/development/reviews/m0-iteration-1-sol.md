# M0 / iteration 1 — independent Sol review

- Review author: independent GPT-6.1 Sol High review agent, 2026-11-09; not the candidate author. This fresh task authored only this report.
- Candidate author: primary implementation agent `/root` (GPT-6.1 Sol High).
- Disposition: **ACCEPT**, scoped to the frozen M0 documentation below. No blocker or major finding. No mandatory M0 correction.
- This is one council disposition, not milestone self-approval or a substitute for the independent Opus disposition. Changes to any manifest-covered file invalidate this pass.

## Frozen inputs and verification

Read `AGENTS.md`, `PLAN.md`, `PRODUCT.md`, `docs/development/progress.md`, `docs/council/decision.md`, and `docs/council/assessment-notes.md`. Verified SHA-256 using PowerShell `Get-FileHash -Algorithm SHA256`; all three manifest entries match.

Manifest: `docs/development/reviews/m0-iteration-1.manifest`  
Manifest SHA-256: `4D1BB2A283F33430A4D69B3A82C0FE9C0C57A8D889D342268B0285527EC43BD8`

| Frozen file | Verified SHA-256 |
| --- | --- |
| `PLAN.md` | `C23D79B407CFA843F3AAC071BA40EF5DA5BF481A17E8101F7091F425F91B0B14` |
| `AGENTS.md` | `269BEF4A647ED57C98F770526820761B504F4C14C035BD3A1C9B4747EB19F40C` |
| `PRODUCT.md` | `15CB07B3A2145E936E2B8419BFE96AB01C4CC2BA2913B23D61356A1E132F758A` |

The following were supporting context, outside the supplied manifest. Their observed hashes identify exactly what was read; this disposition does not independently approve their historical conclusions or future ledger updates.

| Context file | Observed SHA-256 |
| --- | --- |
| `docs/development/progress.md` | `67A304B1074D022C499CDE30D2F656E374B071AE5FAE947F9829F809C4DD5661` |
| `docs/council/decision.md` | `8AC659F51ADACFE69D746B1F6A88E1E52177AE223A6704C3BF75CEF141BCF265` |
| `docs/council/assessment-notes.md` | `757A63E68F14587A2E04B13949706EB0181819C8538121227FB94437410E6445` |

## Findings

No blocker or major findings. The following are **minor, nonblocking suggestions for subsequent candidate definition**, not conditions requiring M0 edits or another cosmetic review cycle.

1. **Make the renderer stop/go boundary executable.** `PLAN.md`, M1, line 47 refers to “the timebox” without defining it. Minimum refinement: record a bounded engineering budget in the M1 packet before composition exploration, with a stop/pivot report if the GPU proof cannot pass. This prevents open-ended interop work; it does not require selecting WinUI now or adding the entire product to M1.
2. **Probe the risky text/input path before broad UI investment.** `PLAN.md`, M2/M5, lines 58 and 74 cover early keyboard/UIA and later complete RTL/IME/text-scale flows. The feasibility decision's “Prototype and stop/go criteria” also calls for an early editable-field and enlarged-text probe. Minimum refinement: schedule one representative text/input/RTL/scale probe during renderer validation or M2, before expanding custom controls. Full product coverage can remain M5. Early keyboard/Narrator requirements already make the current plan sufficient to start M1.
3. **Specify what the lifecycle sample proves.** `PLAN.md`, M1, lines 46 and 52 require independent window close and repeated launch/exit resource release but leave process lifetime and repetition methodology open. Minimum refinement in the M1 packet: define whether closing either/last surface exits or leaves the host resident, select a finite repeat count, and record process termination plus handle/private-byte observations after warm-up. Also include failure after creating the first window/composition resource. This turns the existing cleanup requirement into reproducible evidence without imposing the later full churn budget on M1.

## Engineering assessment

M1 is a realistic bounded first implementation: a pinned Cargo build, two primary-monitor surfaces, one real interactive state, composition proof and lifecycle evidence. It explicitly postpones dock integration, multi-monitor rendering and media work. The installed MSVC/SDK baseline is sufficient to attempt a local Rust bootstrap; missing Rust is an implementation prerequisite, not a planning blocker. The Rust path does not need a .NET SDK. A later WinUI pivot must establish its own SDK/dependency setup and evidence rather than assume the fallback is installed.

The local bootstrap instructions require isolated tooling, pinned versions, provenance and unchanged global PATH. Implementation should keep both Rust installation/cache homes local and avoid installer-driven system changes; this is how to satisfy the existing constraints, not a new approval request.

Thread/lifecycle requirements are appropriate at planning depth. M1 explicitly requires COM/thread ownership, RAII, graceful shutdown and non-unwinding callbacks. M2 adds one shared model, narrow event subscriptions, bounded reconciliation, generations/coalescing and unhooking. The referenced architecture further requires callbacks to enqueue bounded work, apartment-owned COM objects and stale-result rejection. M3/M4 add topology/session replacement and teardown scenarios. Exact apartments, callback-state lifetime, shutdown ordering and partial-initialization cleanup must be reviewed against the actual chosen APIs in M1; no future code is certified here.

The review protocol records authors and hashes, requires fresh independent tasks after changes, bars fix authors from approving their fixes, and requires both council models. Its explicit blocked/pivot outcome prevents the “no justified improvement” stop rule from bypassing an unmet gate. Required checks and empirical evidence cannot be replaced by a static pass. Performance targets remain targets, and matched workloads, uncertainty and compositor/helper costs are recognized. This provides measurable progress without requiring unlimited stylistic iterations.

## Evidence limits and disposition

This was a documentation and input-integrity review only. No toolchain installation, build, runtime, accessibility exercise or benchmark was performed; no external lookup was necessary. Environment availability is taken from the recorded inspection, not independently revalidated. Production code-style review is inapplicable to M0; `AGENTS.md` still requires independent clean-code-guard review when nontrivial production code exists.

**ACCEPT M0 for these exact inputs.** M1 may start after the other independent council disposition and required ledger recording satisfy the M0 gate. This does not accept any future implementation, waive required runtime evidence or establish a performance advantage.
