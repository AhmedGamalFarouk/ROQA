# ROQA project rules

Read `PLAN.md`, `PRODUCT.md` and `docs/development/progress.md` before working.

## Execution and Review Architecture

To maximize development velocity and prevent rate-limit exhaustion, ROQA follows a tiered execution and review model:

- **Tiered Model Routing:**
  - **Tier 1 (Architecture & Milestone Reviews):** Claude Opus 5.5 High / GPT-6.1 Sol High — reserved for milestone gates, architecture decisions, and deep multi-model council evaluations.
  - **Tier 2 (Feature Implementation):** Claude Sonnet 5.5 / GPT-6.1 Sol — fast, quota-resilient implementation of features, refactors, and unit tests.
  - **Tier 3 (Mechanical Git & Verification):** GPT-6.1 Sol (low effort) / Orchestrator — git operations, fast-forwards, pushes, running `scripts/verify.ps1`, formatting, and progress logging. **Never use Tier 1 models for routine git or shell execution.**

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
- Keyboard navigation, hotkeys and screen-reader (UIA/Narrator) support were deliberately removed by user decision (2026-11-13, K1: "Remove all of it"). Add none, and treat reviewer findings about them as out of scope. Native UI still preserves text scaling, RTL/IME in text fields and reduced-motion needs as scoped by the milestone.

## Standing worker rules

Briefs point here instead of repeating these.

- No native/GUI run without a fresh, explicit desktop-free user confirmation via the Chief, plus a foreground-window check. Scripts inject input and move the cursor; prior permission never carries over. Stop only the owned test process, preferably with `--quit`. Settings, display, audio, sleep, Explorer restart, forced TDR and remote-session switches each need separate authorization.
- Never stage or commit the user's own `.gitignore` modification. Never use `git add -A` or `git add .`; stage exact paths.
- Pinned project-local tooling only; never modify global PATH.
- No system changes: global hooks, elevation, startup registration, shell/taskbar mutation.
- Encodings: Rust LF, `progress.md` CRLF, scripts keep their existing encoding (`.gitattributes` enforces this).
- Commit messages end with the co-author trailer the harness specifies; use `git commit -F <file>`; never force-push or skip hooks.
- Don't invent results; keep NOT RUN and deferred items labelled.
- Never build during a measurement.
- Land with `scripts/land.ps1` (stale check, verify, binary SHA, exact-path staging, fast-forward push); use `-DryRun` first.

## Report and ledger size

- A worker's final report is 15 lines or fewer: what changed, test count, binary size and SHA, manifest SHA, deferred items. Paths and hashes, no prose recap.
- `progress.md` entries are 10 lines or fewer and link to the packet for detail.
- Workers read only the last ~60 lines of `progress.md`, never the whole file.
- Manifests list only the files changed by the candidate plus its packet, not the whole tree.

## Parallel work

One slice at a time, or parallel slices only when their files are disjoint. If they overlap, land one first, then start the next from the new `main`. This avoids the double rebase-and-re-review paid for on slices 4-6.

## Feature pipeline

- One batch per worker, from `docs/development/features-queue.md`. Sonnet implements; Gemini 3.8 Flash does research, triage and mechanical landing.
- Briefs are 20 lines or fewer and point to the queue doc.
- Serialize batches that touch `host.rs`, `graphics.rs` or `layout.rs`.
- `verify.ps1` is the gate; reviews only at milestone gates. Reports are 10 lines or fewer.
