**Verdict: REVISE.** Independent read-only clean-code-guard review; I authored no implementation. Counts: **1 critical/major, 1 important/minor, 0 nits.** Runtime and milestone acceptance remain uncertified.

All **29 manifest entries matched**, including `target/release/roqa.exe`, and were reverified after review.

- Input manifest SHA-256: `ba21d0119728a508c189e7094111818c92339e085dac1c887c75e8fd4045fbb1`
- Candidate2 ZIP SHA-256: `9fce5e550db7b7629c4abdf606c24f835b2c45d617327e8db922c973bdd230e9`

The ZIP was read in memory without extraction. After normalizing line endings, production differences are confined to `graphics.rs`, `host.rs`, `layout.rs`, `main.rs`, `native_window.rs`, and new `motion.rs`. Implementation author is recorded as `/root`.

1. **CODE-SOL-001 — Major / critical correctness: the body HWND still intercepts part of the shadow.**  
   Evidence: [layout.rs:64](C:/Users/Ahmed/.t3/projects/roqa/src/layout.rs:64), [layout.rs:78](C:/Users/Ahmed/.t3/projects/roqa/src/layout.rs:78), [native_window.rs:129](C:/Users/Ahmed/.t3/projects/roqa/src/native_window.rs:129).

   `notch_region` extends `body.ears` across the body’s **entire height**, and `notch_hit` similarly treats that rectangle as hovering the notch. At 96 DPI, fully open, the body spans x=40–360 and its input rectangle spans x=24–376. Point **(30,40)** is outside the drawn silhouette—the ears end near y=16—but remains inside both the body HWND’s region and hover predicate. Consequently, a shadow-only pixel can consume an underlying application’s click and hold the panel open. Making the separate shadow HWND transparent does not bypass the body HWND above it.

   **Fix:** derive a shaped input region from the same concave-ear and rounded-body geometry used for drawing; make hover testing follow that silhouette with the intended 4-DIP slop. Do not substitute `HTTRANSPARENT` alone for cross-process pass-through. Extend subsequent runtime evidence to side bands and rounded corners during rest, opening and closing. The current [smoke.ps1:190](C:/Users/Ahmed/.t3/projects/roqa/scripts/smoke.ps1:190) probe checks only the lower band at y=100, which misses this defect.

   Principle: AI failure mode 13, plausible but incorrect geometry.

2. **CODE-SOL-002 — Minor / important robustness: unexpected compositor-clock results bypass the fallback wait.**  
   Evidence: [host.rs:698](C:/Users/Ahmed/.t3/projects/roqa/src/host.rs:698).

   `if wake != WAIT_FAILED.0` accepts every other result. Microsoft documents an immediate `STATUS_GRAPHICS_PRESENT_OCCLUDED` return when the display is off. That result reaches the outer loop without another blocking wait, repeatedly repainting for the remaining animation duration. This is bounded by the tween duration, but wastes work while nothing can be displayed. Other unexpected status values also bypass the advertised fallback.

   **Fix:** distinguish supported event/clock/timeout results from occlusion and failure statuses; route unsupported results through the bounded message wait or suspend rendering appropriately. Microsoft’s function reference and compositor-clock example describe return values inconsistently, so verify that distinction explicitly rather than treating ordinary clock ticks as errors.

   Principle: AI failure modes 6 and 13, external API contract verification.

**Other review conclusions:** Canvas-before-HWND teardown, surface-before-device ownership, region ownership transfer, event unsubscription and existing callback unwind barriers show no additional material regression in the reviewed delta. Worker capacities remain unchanged; settled animation returns to the blocking wait.

I do **not** find a categorical layered-window/DirectComposition incompatibility: Microsoft explicitly permits layered HWND composition targets. The exact layered + transparent + no-redirection combination still needs fresh visual evidence and actual underlying-process click delivery; `WindowFromPoint` alone cannot certify both.

The documented always-animate decision and deferred bounded per-frame allocations are not findings.

Checklist coverage: A naming/functions—no additional finding; B comments/formatting—no additional finding; C SOLID—no additional finding; D DRY/KISS/YAGNI—no additional finding; E AI failure modes—CODE-SOL-001 and CODE-SOL-002.

No files were edited; no builds, tests, GUI actions or subagents were run. **clean-code-guard: 0 fixed, 2 flagged for author.**
