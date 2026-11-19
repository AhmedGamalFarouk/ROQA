# ROQA: project context (single source of truth)

Every agent reads this file first. Briefs point here by path and never paste it. If a brief conflicts with this file, this file wins unless the brief quotes the owner.

## What it is
Native Windows 11 dock + notch in Rust: Win32, DirectComposition/Direct2D, some GDI text. One ~1.5 MB exe, no dependencies beyond Cargo.lock.
Owner: Ahmed (they/them), does not read code. Report in plain language.
One folder, one branch: work on `main` in this checkout. No branches, no worktrees.

## Priorities, in order
1. Performance and tiny footprint: single host (UI) thread (checked 2026-11-16: of ~23 threads at idle, ROQA starts only two parked workers, icons and window source; the rest are the GPU driver and the Windows thread pool), event-driven, no idle timers or threads, one motion clock (`src/motion.rs`), frames only while animating, in-app fast motion respected.
2. Smallest complete change. Reuse what exists; no new abstraction, option or config nobody asked for.
3. Brand consistency (below).

## Hard rules (owner decisions, do not reopen)
- ROQA ignores Windows' Animation effects switch, and the dock and notch have no Narrator or UI Automation support (owner decision 2026-11-19). Do not add either.
- No size limit (owner decision 2026-11-16): `verify.ps1` reports the exe size; put the byte delta in every report. Performance rules above still hold.
- Live tests and screenshots happen on the second (non-primary) monitor only, so the owner's main screen is left alone (owner decision 2026-11-18). Never place ROQA, Settings, a backdrop or the mouse on the primary monitor for a test.
- Answers to the owner are concise (owner decision 2026-11-18): short plain lines with only what changed, what is left and what they must decide; no recaps, no detail they did not ask for.
- No new dependencies.
- Never touch, delete or rebuild anything under `.tools`. Never run rustup or `scripts/bootstrap.ps1`.
- Pushing `main` to origin needs no approval (owner decision 2026-11-16): push after a commit that passes `verify.ps1`. Never force-push, rewrite pushed history, publish releases or delete remote things. Only the Chief commits unless the brief says otherwise.
- Several threads may share this checkout. Stage and commit only the paths you changed (`git commit -- <paths>`), never `git add -A`; if `verify.ps1` fails in files you did not touch, another thread is mid-edit: wait a few minutes and retry. Before pushing, `git pull --rebase origin main` if the push is rejected.
- Never commit `cook/refs`, `cook/stills`, `cook/compare` (private screenshots).
- Workers on Gemini never edit `src/`.
- Threads: when a thread's work is done and its report is read, the Chief settles it in T3 Code (t3_thread_organize); workers never leave a finished thread unsettled.
- Docs: a report, research or scratch .md file that is no longer needed is deleted once its findings are acted on or copied into AGENTS.md, cook/ROUNDS.md or FEATURES_TO_ADD.md; the owner's own notes and AGENTS.md, cook/BRAND/RUBRIC docs are never deleted. Check git status first: delete untracked ones with plain rm, tracked ones with git rm in the same commit.

## Commands
- Cargo only through `scripts\cargo.ps1 <args>`.
- Gate: `scripts\verify.ps1` (fmt, clippy -D warnings, tests, locked release build, size report). A change is not done until it passes.
- Run: `scripts\run.ps1 start` (builds, then launches) and `scripts\run.ps1 quit`.
- Captures: `cook\tools\run-capture.ps1 -Round rNN`, compare with `cook\tools\compare.ps1`. See `cook\tools\README.md`. Never write a new capture script. A full run takes about 6 minutes and returns on its own; run it in the background. If a pass fails with "Settings not found", ROQA crashed: check `Get-WinEvent` Application id 1000 before blaming the tool. Capture stores use fixed sample data (weather shows 3 hours, 29 degrees): that is the fixture, not a bug. ROQA and Nusa both draw at top-centre: the tool quits both, shows a gray backdrop and restarts both.
- Reuse a round's stills when `git rev-parse HEAD` and the exe SHA256 match its `provenance.json`.

## Reading code (token rule)
- The `graft` command is not installed in agent shells (checked 2026-11-16): do not try it. Read the per-file card `graft/src/<file>.md` for the symbol list, then grep the symbol for its real line. Cards are not regenerated, so their line numbers drift; trust the names, not the numbers.
- `host.rs` keeps the `Desktop` struct, start-up and the message loop; its methods by job live in `src/host/{notices,settings_bridge,menus,notch_clicks}.rs` (same `impl Desktop`, `pub(super)`).
- Read a span, not a file. `host.rs` (about 5,100 lines), `graphics.rs`, `settings_window.rs`, `layout.rs`, `media.rs` are never read whole.
- Read a full file only when it is under ~300 lines or you are rewriting most of it.
- Docs: read the summary here; open the source doc only for the section you need.

## Brand (approved; full spec `cook/brand/BRAND.md`, board `cook/brand/board.png`)
- Companion-like and playful: calm, caring, springy when it greets you.
- One capsule shape grammar (radius = height/2; tiles = parent radius minus inset; never a sharp corner or hairline box).
- One accent, Lamp `#FFB454` (Light theme `#9A5200`). Accent only on: eyes, the one primary action, progress fill, selected indicator, today marker, prayer sun.
- One outline icon style: 20 px grid, stroke 1.75, round caps, accent only when active.
- Three motion curves in `src/motion.rs`: settle, pop, slide. Nothing else.
- Type: Segoe UI Variable, sentence case, tabular numerals, no uppercase labels.
- Voice: warm, brief, 7 words or fewer, no exclamation marks, no em dashes, no emoji.
- Never: purple gradients, glow orbs, decorative particles, shockwave rings.
- Five themes (Classic, Light, Midnight, Frosted tint, Liquid glass) change surface and ink only.
- Reference app: Nusa (`cook/refs/R*.png`). Take ideas, never copy.

## Where state lives
- Design QA ledger: `cook/ROUNDS.md` (short; old rounds in `cook/archive/`).
- Critic rubric: `cook/RUBRIC.md`.
- Feature ideas: `FEATURES_TO_ADD.md` (local, untracked wish list; not a plan).
- History only, never needed for a task: `cook/archive/` (old briefs, Nusa shot notes, old fix lists). If a rule there differs from this file, this file wins.

## Handoff contract
Brief (Chief to worker), at most 15 lines:
```
Read AGENTS.md first. Role: implement | capture | critic | test.
Task: <one paragraph, the outcome wanted>
Scope: <files or surfaces; what not to touch>
Done when: <checkable criteria>
Report: the format in AGENTS.md.
```
Report (worker's final message), at most 12 lines, nothing else:
```
RESULT: done | partial | blocked
CHANGED: <files, or commit sha>
VERIFY: verify.ps1 pass|fail|not run; exe <bytes> B
EVIDENCE: <paths to stills, logs or numbers>
OPEN: <what is left, risks, decisions for the owner>
settle: done|n/a
```
The Chief reads only that final message (`t3_thread_read` limit 1), never a transcript. Anything longer goes in a file whose path is in EVIDENCE. Workers do not start sub-agents.

## Models (cost rule)
- Capture, running tests, regression eyeballing: Gemini 3.8 Flash (provider `antigravity`, model `gemini-3.8-flash-high`). If it stalls (it did twice on 2026-11-15), switch to Codex Luna (`codex`, `gpt-6-luna`, medium) without waiting.
- Critics (owner decision 2026-11-16): two per scored round, Sol 6.1 (`codex`, `gpt-6.1-sol`, medium) and Opus 5.5 (`claudeAgent`, `claude-opus-5-5`, medium). Tie-breaker only when they differ by more than 1.0.
- Code changes: the model the owner names. Codex workers have no ponytail skill: apply "smallest complete change" by hand.
