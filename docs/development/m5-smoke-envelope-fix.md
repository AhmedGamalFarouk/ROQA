# M5 smoke expansion probe: harness-only candidate

Author: implementation sub-agent (GPT-6.1 Sol), root checkout `main`, base `238c2c853ee2ce4f13acfe6dbac106601504698f`, 2026-11-11. No commit/push or independent review; milestone acceptance unchanged.

## Evidence and diagnosis

Original evidence: `artifacts/m5-smoke-sol-1/{smoke.json,suite.log,runtime.log,preflight.json}`. The expansion probe `(100,24)` received 0 clicks while region bounds changed `102,0,236,48` -> `87,0,266,56` over 13 ms. Bounds membership changes false -> true. At y=24 the later shape is on its straight body side (past the <=16-DIP ears and above the bottom corner); its left body edge is x=100. The first WindowFromPoint lookup cannot certify a later injected click. Production regression is unproven.

## Change and static proof

Only `scripts/smoke.ps1` changes executable behavior. The expansion probe uses a fixed envelope rather than the transient region's left edge. For the opening spring, the global step-response peak is `1 + exp(-pi * .65 / sqrt(1 - .65^2)) = 1.06807664510188`; opening from the .07 intent swell lowers that peak. The harness conservatively bounds progress by 1.1: width <=337 DIP, ears <=16 DIP. Every silhouette row has left edge >= floor((windowWidth - roundedWidth)/2) - roundedEars, since both the circular-ear and bottom-corner offsets are nonnegative. The probe is at least 2 physical pixels farther left, within the notch HWND. At 96 DPI: envelope left=35, probe x=33, y remains the observed half-height (24 in the original sample). Replacement membership is false -> false in both historical bounds, also outside the entire overshoot envelope. The envelope is computed before the timed observation/click path.

The 48..64-DIP intermediate observation, two-second timeout, post-click nondecreasing height below 76 DIP, fixture WindowFromPoint assertion, and exactly-one actual delivered click assertion remain. New per-click fields record before/after bounds membership and the expansion envelope. Bounds containment is explicitly distinguished from shaped-region membership; exclusion proves the point is outside the region. The phase is now `expanding exterior margin`; expansion shadow overlap is `NOT ESTABLISHED`, so no shadow-coverage claim is made for it. Production animation, input regions, teardown/error handling and all other cases are unchanged.

## Verification (author checks, no GUI)

- `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File artifacts/m5-smoke-envelope-fix/static-check.ps1`: PASS on Windows PowerShell 5.1.26100.9818. AST parsing and execution of only the two pure helpers; source-assumption guards; 16,814 geometry samples across 14 DPIs from 48 to 768; analytic all-row bound; historical/replacement membership; half-open rectangle edges; script UTF-8 BOM/CRLF. Evidence: `artifacts/m5-smoke-envelope-fix/static-check.{ps1,log}`. No native probe type loaded, GUI or input executed.
- `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File scripts/verify.ps1 -EvidenceDirectory artifacts/m5-smoke-envelope-fix/verification`: PASS, checks exit 0 and build exit 0. `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`: **175 passed, 0 failed, 0 ignored**; locked release build PASS. Project-local pinned toolchain wrappers used. Exact commands/status and stdout/stderr: `artifacts/m5-smoke-envelope-fix/verification/`.
- Release binary: `target/release/roqa.exe`, **999424 bytes**, SHA-256 `A329BF01972840FC1EBAE04792E00B0E6CEFF1F0093991BCC22831433D70622C` (unchanged from original smoke binary).
- Candidate manifest: `docs/development/m5-smoke-envelope-fix.sha256`, only smoke script and this packet; mutable progress ledger excluded. Original evidence retained untouched. Script BOM/CRLF and progress CRLF preserved; user's `.gitignore` preserved, no staging.

## Open evidence

Smoke rerun **NOT RUN**: no native/GUI run or input injection authorized; static checks cannot certify live click delivery or timing. Existing `Accessibility cleanup: The parameter is incorrect. (0x80070057)` appears twice in the original runtime log and remains separately **unresolved**; no teardown error is changed or suppressed. Expansion shadow overlap remains unestablished. No system/settings changes or smoke retry.
