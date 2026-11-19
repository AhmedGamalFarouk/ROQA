---
name: council
description: Two-model review council for ROQA (Sol 6.1 and Opus 5.5). Trigger when the owner says "council", or asks for a council review, second opinion or review round on code, a plan or a design.
---

# Council (ROQA)

Adapted from t3dotgg/tstack `skills/council`. AGENTS.md wins on any conflict.

## Before the council
Fix what is already known first, through an implementer thread (the Chief does
not edit code): open review notes, failing `scripts\verify.ps1`, known bugs.
Give the council the current state of `main`. If the owner wants to throw the
work away and explore new ideas, skip fixes to code that will be replaced.

## Members (fixed, owner decision 2026-11-20)
Exactly two, both at high reasoning effort, both read-only:
- Sol 6.1: provider `codex`, model `gpt-6.1-sol`, reasoningEffort `high`.
- Opus 5.5: provider `claudeAgent`, model `claude-opus-5-5`, effort `high`.

The owner may name other members for one run, and that wins for that run. If
a member is unavailable, say so to the owner. Never swap in another model.

## Starting them
Start both in parallel with `t3_thread_launch` (shared checkout on `main`, no
worktree), titled `Council rN: <topic> (Sol)` / `(Opus)`. Use the AGENTS.md
brief format, 15 lines or fewer, Role: critic, and include:
- the request, the commits or files in scope, and what "done" looks like;
- the skills: /review and rust-best-practices for code (ponytail lens:
  smallest complete change), cook/RUBRIC.md for visuals; no others;
- "Read-only: do not edit, build, commit or start ROQA. Live checks, if
  needed, use `scripts\run.ps1 start -Test` on the second monitor only.";
- "Score out of 10. List findings most severe first, each with file:line, the
  failure and the smallest fix. Use the AGENTS.md report format; put the full
  list in a file under the scratchpad or `docs/council-rN-<member>.md` and its
  path in EVIDENCE."

## While they run
Do not wait idle. When one report lands, read it (`t3_thread_read` limit 1,
then its EVIDENCE file), check each finding against the code at once, and
sort it: fix, reject (with a one-line reason) or ask the owner. Do not
start fixes on files the other member is still reviewing. Settle each member
thread after its report is read.

## Fixing
Hand accepted fixes to one implementer thread (the model the owner names,
else Sonnet 5.5) with the usual skills: ponytail full, /verify,
rust-best-practices. Once it passes verify and is pushed, start a fresh
round: both members again, new threads. Each full pass by both members is
one round.

## When to stop
Stop when both members score 9/10 or more with no blocking finding, or after
3 rounds. Then report to the owner whatever is still open, as a decision.
Fight scope creep: a finding outside the original request goes to
FEATURES_TO_ADD.md, not into the fix list.

## Reporting to the owner
Concise, plain language, no code (AGENTS.md). One short table, then only
what changed, what is left and what they must decide:

| Member | Round 1 | Round 2 |
| --- | --- | --- |
| Sol 6.1 | 7/10 | 9/10 |
| Opus 5.5 | 8/10 | 9/10 |

Write `no score` if a member gave none. Never invent one. There are no PRs
(work lands on `main`), so the table goes in the reply. Delete the
`docs/council-*.md` files once their findings are fixed or recorded (AGENTS.md
Docs rule).
