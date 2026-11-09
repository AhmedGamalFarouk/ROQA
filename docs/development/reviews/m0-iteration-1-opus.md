# M0 iteration 1 — independent review (Opus 5.5 High)

- Reviewer: Claude Opus 5.5 High, delegated review task (read-only except this report). Did not author any reviewed file.
- Candidate author: primary agent `/root` (GPT-6.1 Sol High), per `docs/development/progress.md` and `PLAN.md:3`.
- Date: 2026-11-09.
- Sol M0 report: not read before or while forming this review.

## Frozen inputs (manifest `docs/development/reviews/m0-iteration-1.manifest`)

| File | Manifest SHA-256 | Recomputed | Match |
| --- | --- | --- | --- |
| `PLAN.md` | `C23D79B4…F91B0B14` | `c23d79b4…f91b0b14` | yes |
| `AGENTS.md` | `269BEF4A…EB19F40C` | `269bef4a…eb19f40c` | yes |
| `PRODUCT.md` | `15CB07B3…132F758A` | `15cb07b3…132f758a` | yes |

Context read but **not** covered by the manifest (see MIN-1): `docs/development/progress.md` (`67a304b1…c4dd5661` when first read), `docs/council/decision.md` (`8ac659f5…41bcf265`), `docs/council/assessment-notes.md` (`757a63e6…410e6445`). This disposition applies only to the three manifest files at the hashes above; all three were re-verified unchanged when this report was finalized.

During this review `progress.md` changed to `40f3cca6…23063c74`. The ledger now records Sol's ACCEPT and lists Sol's non-blocking M1 refinements. I saw that update only after all findings below were drafted. I did not open the Sol report, and I changed no finding because of it. The mid-review change to an unhashed input illustrates MIN-1. Sol treats the renderer timebox and the text/RTL probe as non-blocking, while this review rates them material (MAJ-1, MAJ-3). That severity split is exactly the case MAJ-4's missing disagreement rule leaves unresolved.

## Evidence and limits

- Read-only environment checks (no installs, no settings changes): `cargo`, `rustc`, `rustup` not on PATH; no `~/.cargo` or `~/.rustup`. `vswhere` reports VS Build Tools 2022 17.14.41; `VC/Tools/MSVC/14.44.35207/bin/Hostx64/x64/cl.exe` exists; Windows SDK `10.0.26100.0` with `um/x64/kernel32.lib` present. `dotnet --list-sdks` returned nothing. This matches the ledger's environment claim.
- No `.gitignore` exists yet; workspace also contains an untracked empty `graft/` directory outside the manifest (not reviewed, not attributed).
- External facts cited below from prior knowledge, not re-fetched in this task: `rustup-init --no-modify-path` flag; Rust 1.81.0 aborts on panics escaping non-unwinding `extern` ABIs. The implementer must verify against primary docs when bootstrapping.
- No code exists; nothing was built or run. This is a planning-gate review only; future runtime evidence is assessed for whether the plan *requires* it, not whether it exists.

## Overall

The plan is honest about what is unmeasured, keeps the taskbar intact, keeps M1 primary-monitor-only, and has a sound independence rule. It can start. What it lacks is (a) a verifiable M1 contract — several M1 deliverables have no acceptance check and the pivot rule references an undefined timebox, (b) required rendered evidence for the graphics proof, (c) scheduling of the decision's stack-deciding risks before the feature milestones, and (d) a way for the two-reviewer loop to terminate when reviewers disagree. All four are small text edits; none requires a user decision.

## Findings

### Major (mandatory before M0 acceptance)

**MAJ-1 — M1 acceptance does not verify several M1 deliverables; pivot timebox undefined.** `PLAN.md` § M1 (lines 47–52).
- "If composition is not viable in the timebox" — no timebox is defined anywhere in `PLAN.md`, so the pivot rule cannot fire.
- "one meaningful interactive state" is undefined, so it cannot be accepted or rejected.
- Deliver lists no-focus-steal, keyboard exit, independent open/close of each window, DPI awareness and "no invisible full-monitor input surface"; the Acceptance paragraph checks none of them.
- Keyboard reachability conflict: a non-activating notch/dock (the natural way to satisfy "does not steal focus") is not reachable by Alt+Tab or click-activation, so "Escape when focused" may be unreachable for a keyboard user. The plan must name an exit path that works without mouse focus.
- "Repeated launch/exit releases resources" is vacuous across process exits (the OS reclaims everything). The meaningful check is in-process create/destroy cycles.

Minimum correction (text only): in § M1 Acceptance add (1) a timebox for the composition proof, stated as either calendar time or a number of candidate iterations, and the rule that on expiry the author records a pivot or rescope candidate for council review; (2) a one-line definition of the interactive state (for example, pointer hover/press changes a composition property and reverts); (3) checks that the foreground window is unchanged after launch, that each window's rect matches the expected primary-monitor geometry and is no larger than intended, and that each window can be closed and reopened independently; (4) a documented graceful exit that works without mouse focus (for example, a second invocation that signals the running instance). The smoke test uses that path, requires exit code 0, and counts a forced kill as a failure; (5) N in-process window/composition create-destroy cycles with handle, USER and GDI object counts (`GetGuiResources`) and private bytes recorded before and after. Keyboard *focus entry* into the surfaces can stay in M2.

**MAJ-2 — Graphics proof can be accepted without rendered evidence.** `PLAN.md` § M1 line 47 vs line 52.
Line 47 says not to "call the graphics proof passed" without composition. Line 52 then makes rendered capture optional ("if supported … stays an open limitation"). Together with `AGENTS.md` ("No milestone is accepted on … missing required runtime evidence") this is ambiguous: M1 could be accepted with the core graphics claim unverified. Window existence (an HWND) does not prove composed pixels.
Minimum correction: state that rendered evidence for the composition proof is **required** for M1 acceptance. Use a screen-region capture of the window rect with an automated pixel or colour assertion for the solid content and for the interactive state. If no capture method works in this environment, the graphics gate stays OPEN and M1 is not accepted, unless the user waives it in writing.

**MAJ-3 — Stack-deciding risks from the council decision are deferred past the feature milestones or not scheduled.** `PLAN.md` §§ M2–M5 vs `docs/council/decision.md` lines 64–66, 84, 100.
The decision makes the Rust-vs-WinUI pivot depend on custom text, input, UIA and composition interop. It names "building all the widgets before proving renderer, input, accessibility and mixed-DPI lifecycle" as the highest-cost mistake. `PLAN.md` proves UIA in M2 (good). Text rendering, Arabic/RTL shaping, IME and text scaling first appear as acceptance in **M5**, after dock, multi-monitor and media are built. Mixed DPI arrives in M3. D3D/composition **device loss**, **lock/unlock** and **source-app hang**, all listed by the decision as required tests, appear in no milestone. Reduced motion appears only in `AGENTS.md`.
Minimum correction: (1) add to M2 acceptance a text proof: DirectWrite (or the chosen text path) rendering an Arabic RTL title correctly, honouring the system text-scale setting, and one IME-capable input route (an OS edit control is acceptable). (2) Add an explicit, council-reviewed **stack go/no-go record** at the end of M2, before M3 starts, citing the decision's pivot criteria. (3) Add device-lost recovery, lock/unlock and source-app-hang cases to M3 or M4 acceptance, and reduced-motion behaviour to M4, where the first expand/collapse animation lands. Do not move these into M1.

**MAJ-4 — The review loop has no termination or disagreement rule, so it may not converge.** `PLAN.md` § Independent hill-climbing protocol steps 5–7, § M0 Exit.
- No tie-breaker: if Sol reports a major and Opus disputes it (or the author rebuts it), nobody may adjudicate. The author cannot, and the decision's "chair" role is not carried into `PLAN.md`.
- No recheck scope: each "fresh" reviewer re-reads everything and can raise new minors forever. That is the stylistic churn step 7 tries to forbid, with nothing enforcing it.
- No round cap or escalation.
- Exit wording ("final dispositions recorded") does not require both ACCEPTs on the **same** manifest hash, so an accept on iteration 1 and an accept on iteration 2 could be combined.
- No stable finding IDs, so it cannot be audited whether a round "improved".

Minimum correction: add to the protocol (1) a finding ledger in `progress.md` with stable IDs and status (open, fixed, disputed, withdrawn) carried across iterations; (2) recheck scope: prior findings, the diff and regressions caused by it, with new findings on unchanged content admitted only at blocker/major severity and with evidence; (3) minors never block acceptance, and the author may defer them with a recorded reason; (4) a disputed blocker/major goes first to the author's written evidence and the reviewer's recheck; if it is still disputed, it goes to the user, not the author; (5) after a fixed number of recheck rounds (for example 3) without convergence, escalate to the user; (6) acceptance requires ACCEPT from both reviewers on the identical final manifest hash.

### Minor

**MIN-1 — Manifest scope is narrower than the normative inputs.** `docs/development/reviews/m0-iteration-1.manifest`; `PLAN.md:7`, `PLAN.md:76`.
`PLAN.md` delegates its qualifications and gates to `docs/council/decision.md`, and the ledger lists itself as an input, yet neither is hashed. Correction: hash normative referenced docs (`docs/council/decision.md`, `docs/council/assessment-notes.md`) in future manifests, and state in § Review packets that `progress.md` is a ledger updated after review and is excluded by design.

**MIN-2 — Bootstrap mechanics that protect the "leave global PATH alone" rule are unstated.** `PLAN.md:14`, `AGENTS.md:11`.
Default `rustup-init` writes the user PATH (HKCU). Correction: in § Working defaults, or as the first M1 item, require: `rustup-init` downloaded from the official host with its published SHA-256 verified; run with `--no-modify-path`; `RUSTUP_HOME`/`CARGO_HOME` under `.tools/`, set only inside a wrapper script; `rust-toolchain.toml` pinning an exact version plus `rustfmt`/`clippy` and target `x86_64-pc-windows-msvc`; a `.gitignore` covering `.tools/` and `target/` created before bootstrap; and `--locked` on build, test and clippy commands. Record that MSVC 14.44 / SDK 10.0.26100.0 are reused, not installed.

**MIN-3 — Dock placement and work-area behaviour are unspecified.** `PLAN.md:46`.
A "bottom companion dock" could overlap the taskbar, or tempt AppBar registration, which changes other apps' work area (shell-visible state). Correction: in M1, place windows within the monitor work area, with no `SHAppBarMessage` or work-area reservation, and topmost/no-activate tool windows unless reviewed otherwise.

**MIN-4 — Pivot path needs a toolchain the machine lacks.** `PLAN.md:47`; `PRODUCT.md` § Stack.
The WinUI fallback needs a .NET SDK, which is absent. Correction: state that a pivot is a plan amendment with its own reviewed bootstrap (project-local, pinned), never a silent install.

**MIN-5 — clean-code-guard has no place in the packet.** `AGENTS.md:12` vs `PLAN.md` § Review packets.
Correction: list an independent clean-code-guard pass as a required section of the M1+ review packets.

**MIN-6 — M1 performance sample needs fixed method fields.** `PLAN.md:52`.
Correction: specify release build, warm-up, sample duration, counters (process CPU time delta, private bytes, handles), tool, OS build and hardware, and a label stating it is an observation, not compared to M5 budgets or to Nectar or Nusa.

### Suggestions (non-blocking)

- Decide the panic strategy at the FFI boundary: `catch_unwind` in `wndproc` and other callbacks that turns a panic into a logged error plus orderly quit, or a documented reliance on abort (Rust ≥1.81 aborts on panics escaping non-unwinding `extern` ABIs). Add one test that proves the chosen behaviour.
- The M1 packet should document the apartment and teardown order for the chosen composition API. For Windows.UI.Composition on Win32 that is STA init, the `DispatcherQueueController` lifetime and shutdown, and composition object release before uninit. For DirectComposition it is device ownership and device-lost handling.
- Declare per-monitor-v2 DPI awareness via the embedded application manifest and handle `WM_DPICHANGED`, even on primary-only M1, since users can change scale while the app is running.
- Independence: the author and the Sol reviewer share a model. `PLAN.md:33` permits this as separate task instances; Opus provides the cross-model check. No change needed; keep recording actual identities.

## What is already adequate

- Taskbar coexistence, and no hooks, elevation, startup registration or registry pin parsing in M1 (`PLAN.md:15`).
- Performance claim discipline: targets are labelled as targets, "no speedup measured" (`PLAN.md:7`), M5 thresholds change only by reviewed amendment, no `EmptyWorkingSet`, and a measurement floor leads to "inconclusive" (`PLAN.md:76–78`).
- Self-approval prohibition and fix-author ineligibility (`PLAN.md:26–28, 33`; `AGENTS.md:5–6`).
- M1 is primary-monitor-only, solid-colour composition, with no feature parity claimed. The scope is realistic once MAJ-1 and MAJ-2 make it checkable.

## Disposition

**REVISE.** Blockers: none. Material (major) items required for M0 acceptance: **MAJ-1, MAJ-2, MAJ-3, MAJ-4**. Minors MIN-1 to MIN-6 are recommended; MIN-2 should land before the toolchain bootstrap. All corrections are plan text and need no new user decision. A fresh independent recheck should be limited to these findings and to any regressions the edits introduce.
