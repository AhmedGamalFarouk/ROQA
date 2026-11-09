# ROQA project rules

Read `PLAN.md`, `PRODUCT.md` and `docs/development/progress.md` before working. The user requires independent Sol High and Opus 5.5 High council review at each milestone, with no agent reviewing its own work.

- Work in bounded candidates. Record author and exact reviewed inputs. Authors can run checks but cannot independently approve their own work.
- Every new review/recheck is a fresh delegated task. Reviewers do not edit implementation files. Any reviewer who authors a fix cannot approve it.
- No milestone is accepted on stale inputs, failed required checks or missing required runtime evidence. Report unknowns honestly.
- Keep the native taskbar available by default. No global hooks, elevation, startup registration, shell mutation or system configuration changes unless the relevant reviewed milestone explicitly calls for them within user scope.
- Use verified Windows/Rust APIs, explicit COM ownership, bounded background work and resource cleanup. Do not unwind through FFI callbacks.
- No copied Nectar/Nusa code or assets. Do not invent product data or performance results.
- Use project-local tooling when possible, leave global PATH alone, pin dependency/toolchain versions and keep generated binaries/caches out of version control.
- Apply clean-code-guard through an independent reviewer for nontrivial production changes. Native UI should preserve keyboard, accessibility, text scaling, RTL/IME and reduced-motion needs as scoped by the milestone.
