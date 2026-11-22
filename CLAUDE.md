@AGENTS.md

## Skill routing

When a request matches an available skill, invoke it via the Skill tool. Answer directly for quick questions or small scoped edits. AGENTS.md wins on conflicts: no branches or PRs, so /ship and /land-and-deploy are not used here; commit and push to `main` as AGENTS.md says.

- Product ideas/brainstorming → /office-hours
- Strategy/scope → /plan-ceo-review
- Architecture/plan lock-in → /plan-eng-review
- Full review pipeline → /autoplan
- Bugs/errors → one read-only check of the cause, then fix it test-first
- Testing behavior → /qa or /qa-only (live tests on the second monitor only)
- Code review/diff check → /review (scored rounds: council)
- Visual polish → /design-review
- Run the checks → /verify (the gate is `scripts\verify.ps1`)
- Save/resume context → /context-save, /context-restore
- Backlog-ready spec → /spec
