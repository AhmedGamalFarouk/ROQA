# ROQA project rules

Read `PLAN.md`, `PRODUCT.md` and `docs/development/progress.md` before working.

## Execution and Review Architecture

To maximize development velocity and prevent rate-limit exhaustion, ROQA follows a tiered execution and review model:

- **Tiered Model Routing:**
  - **Tier 1 (Architecture & Milestone Reviews):** Claude Opus 5.5 High / Gemini Pro — reserved for milestone gates, architecture decisions, and deep multi-model council evaluations.
  - **Tier 2 (Feature Implementation):** Claude Sonnet 5.5 / GPT-6.1 Sol — fast, quota-resilient implementation of features, refactors, and unit tests.
  - **Tier 3 (Mechanical Git & Verification):** Gemini Flash / GPT-6.1 Sol (low effort) / Orchestrator — git operations, fast-forwards, pushes, running `scripts/verify.ps1`, formatting, and progress logging. **Never use Tier 1 models for routine git or shell execution.**

- **Milestone-Level Reviews vs. Intermediate Work:**
  - Work in cohesive, logical feature increments (e.g. whole backend or complete UI panels), avoiding over-fragmentation into micro-slices.
  - For intermediate implementation commits, automated verification (`scripts/verify.ps1`: `cargo fmt`, `clippy -D warnings`, `cargo test`) is the gate. When tests pass and clippy is clean, commit and proceed.
  - Formal independent reviews (Clean Code Guard / Council Review by Sol High + Opus High) occur at **Milestone Gates** (e.g. M3 completion, M4 completion, M5 performance gates), rather than on every microscopic commit.

- **Context Efficiency:**
  - Keep delegated worker briefs concise (scope, branch, acceptance criteria). Do not dump entire constitutions or historical files into prompts; let workers read files on demand via tool calls.
  - Prune git worktrees immediately after landing work to keep the workspace clean.

## System & Code Integrity Rules

- No milestone is accepted on stale inputs, failed required checks or missing required runtime evidence. Report unknowns honestly.
- Keep the native taskbar available by default. No global hooks, elevation, startup registration, shell mutation or system configuration changes unless the relevant reviewed milestone explicitly calls for them within user scope.
- Use verified Windows/Rust APIs, explicit COM ownership, bounded background work and resource cleanup. Do not unwind through FFI callbacks.
- No copied Nectar/Nusa code or assets. Do not invent product data or performance results.
- Use project-local tooling when possible, leave global PATH alone, pin dependency/toolchain versions and keep generated binaries/caches out of version control.
- Native UI should preserve keyboard, accessibility, text scaling, RTL/IME and reduced-motion needs as scoped by the milestone.
