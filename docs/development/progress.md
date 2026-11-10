# Development ledger

## M0 / iteration 1

- Candidate author: primary agent `/root` (GPT-6.1 Sol High).
- Inputs: `PLAN.md`, `AGENTS.md`, `PRODUCT.md`, this ledger, and feasibility context under `docs/council/`.
- State: superseded by iteration 2. Sol ACCEPT, Opus REVISE with four major findings; no production code written.
- Sol review: `reviews/m0-iteration-1-sol.md`, ACCEPT; all three manifest hashes match, no blocker/major. Review task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3Aca4fce1d-e9bc-469b-b51e-3e3b5a1bf276>.
- Nonblocking M1 refinements from Sol: define renderer exploration timebox; schedule representative editable-text/RTL/scale probe no later than M2; define finite lifecycle repetitions, last-window shutdown behavior and partial-initialization failure test in the M1 candidate packet. No manifest-covered plan change requested.
- Environment observed: Windows x64; MSVC 2022 Build Tools 17.14 and Windows SDK 10.0.26100.0 installed; Rust not found on PATH or in the usual user installation; .NET runtimes present but no SDK.
- Verification: repository/environment inspection only. No app build, smoke run or performance result.
- Next: resolve material plan findings, freeze final M0 inputs and obtain independent dispositions; then bootstrap and implement M1.

## M0 / iteration 2 — fix batch / recheck round 1 of 3

- Author: `/root`; changed `PLAN.md` only among iteration-1 candidate files. `AGENTS.md` and `PRODUCT.md` unchanged.
- Opus initial review: `reviews/m0-iteration-1-opus.md`, REVISE; task <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3A130b026a-66fc-4639-8065-6c997a792dd0>.
- The author accepted the stricter acceptance-contract recommendation; no material finding is disputed or waived. Reviewer severity differences do not confer approval on the author.
- All statuses below mean **fix-submitted**, not independently resolved:

| Finding | Submitted change |
| --- | --- |
| M0-OPUS-MAJ-1 | Two-candidate renderer timebox; pointer-enter/leave state; focus/geometry/open-close checks; focus-independent `--quit`; 100 in-process resource cycles after warm-up and partial-init failure |
| M0-OPUS-MAJ-2 | Mandatory actual screen-region evidence and pixel assertions for both composed windows and interaction; inability leaves M1 open |
| M0-OPUS-MAJ-3 | M2 Arabic/RTL/system-text-scale/IME proof and independent stack gate before M3; device loss, lock/unlock and hung-source cases in M3/M4; reduced motion M4 |
| M0-OPUS-MAJ-4 | Stable IDs/statuses, bounded recheck scope, no minor blocking, dispute escalation, three-round cap, both ACCEPTs on identical final manifest SHA |
| M0-OPUS-MIN-1 | Normative council docs included in iteration-2 manifest; mutable ledger explicitly excluded |
| M0-OPUS-MIN-2 | Checksummed official rustup, verified flags, `--no-modify-path`, local homes/wrappers, exact toolchain/components/target, early ignore rules, locked builds |
| M0-OPUS-MIN-3 | Work-area placement; no AppBar reservation; explicit passive window behavior |
| M0-OPUS-MIN-4 | WinUI pivot requires reviewed local pinned SDK setup |
| M0-OPUS-MIN-5 | Independent clean-code-guard identified in M1+ packets |
| M0-OPUS-MIN-6 | Fixed release/settle/sample/counter method and observation-only label |

- Sol's three nonblocking suggestions are covered by the corresponding M1/M2 changes above.
- Evidence: documentation changes only; future runtime checks remain unperformed. Fresh Sol and Opus rechecks must verify the same new manifest and resolve material findings before M1 begins.
- Sol recheck: `reviews/m0-iteration-2-sol.md`, ACCEPT for manifest `30E9BDE5B31564D89EDE3B393F074C19EA008B2681A9073DAE2C33C63D962133`; all prior IDs resolved within this independent review's scope, no material regression. Task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3A9ffd522b-dc20-4993-a14b-e19348fe0f6b>.
- Gate remains pending the independent Opus recheck on the identical manifest. No implementation started and no candidate files changed following the Sol acceptance.

## M0 final disposition / M1 started

- Opus recheck: `reviews/m0-iteration-2-opus.md`, ACCEPT for the identical manifest `30E9BDE5B31564D89EDE3B393F074C19EA008B2681A9073DAE2C33C63D962133`. Task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3Ad6440115-bcc6-4fb3-a2d3-76f6c29d0aa7>.
- **M0 accepted** by both independent reviewers on the same manifest. M0-OPUS-MAJ-1..4 and MIN-1..6 are resolved by independent dispositions. No normative input changed after acceptance.
- M1 candidate 1 author: `/root`. Scope: local Rust bootstrap, primary-monitor DirectComposition shell, focus-independent CLI control, rendering/lifecycle evidence. This is an implementation candidate, not an accepted milestone.
- Bootstrap observation: official archived rustup 1.28.2 checksum `88d8258dcf6ae4f7a80c7d1088e1f36fa7025a1cfd1343731b4ee6f385121fc0`; Rust 1.91.1 and components installed locally. Initial final verification failed after automatic rustup self-update; fixing the wrapper to disable that behavior. No global PATH change was requested.

## M1 / candidate 1 — independent review pending

- Author: `/root`. Packet: `m1-candidate-1.md`. Graphics exploration candidate 1 of the permitted 2; no milestone acceptance claimed.
- Delivered: local pinned Rust/Cargo setup; DirectComposition/Direct2D hardware surfaces; primary-monitor placement and hover state; blocking message/event wait; SID/session-scoped control events; RAII/STA/callback-boundary cleanup; graceful CLI exit; build/run/check/smoke/observation scripts.
- Author verification: bootstrap success, release build, formatter, warnings-denied all-target Clippy, three geometry tests. Final native smoke PASS with 282 assertions; 10 warm-up plus 100 in-process notch cycles while dock remains alive; composed-pixel captures, no focus/work-area change, synthetic DPI, independent close/reopen, last-window exit and failure probes all passed.
- Final release SHA-256: `9411F5548BF9A5E6370F87B0AB4AD090142B3C33948EDA61842BA5C765AE6CA7`. This matches `artifacts/m1/smoke.json` and `artifacts/m1/idle.json`.
- Final resource sample: warm 11,030,528 private bytes / 242 handles / 0 GDI / 7 USER; final 11,194,368 / 245 / 0 / 7, with blocks 1–5 flat. These are workload observations, not general leak-freedom claims.
- Short idle sample: 30.017 seconds; private bytes 11,292,672 to 11,411,456; no CPU-time delta measurable by the endpoint counter. DWM/GPU/power not attributed; no M5 or competitor claim. See raw evidence and limits.
- Failed intermediate checks were corrected, not accepted: manifest linker flags applied to dependency builds; shell argument separator stripped by PowerShell; overlay interference in edge hover screenshot; invalid synthetic DPI fixture with null suggested RECT. The final harness isolates only test-owned windows and uses a valid DPI message payload. No unrelated applications were stopped.
- M0-OPUS-R2-MIN-1/2 addressed in the packet: keep dock alive during notch churn; explicit pass/fail plateau limits and monotonic-growth check. Independent review resolution pending.
- Native test instances exited; no ROQA process intentionally left running. Next: fresh non-author Sol and Opus review on one frozen manifest; Opus has the exclusive live smoke slot and must use a separate evidence directory. No M2 work before acceptance.

## M1 / iteration 1 - Opus review received, Sol review still running

- Frozen manifest remains `76B1A9EC66BF2033B8DD095B51993A4120B398F8ECF229A25FE87914536EFA8B`. No candidate input changed on receipt of this review.
- Independent Opus disposition: **REVISE**, report `reviews/m1-iteration-1-opus.md`; task <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3A12a2b149-2d59-4968-b97e-a1cfe8dc97fb>.
- Open major `M1-OPUS-MAJ-1`: startup/settings geometry uses sign-in system DPI; source needs current primary-monitor/window DPI and a regression check that does not assert stale-DPI reversion as success.
- Minor follow-up set `M1-OPUS-MIN-1..5`: avoid duplicate unnecessary rebuilds on settings broadcasts; distinguish cursor/foreground interference from product failure; strengthen pixel and exact failure-exit assertions; make idle evidence directory configurable; document fullscreen overlap until M3.
- Independent runtime evidence: frozen smoke failed three times amid physical cursor/foreground interference. Opus's separately guarded harness passed 288 assertions, resource cycles plateaued, and both failure probes exited 1. Guarded results support the implementation but do not retroactively make the frozen smoke robust. Actual display-scale changes and mixed-DPI hardware remain untested.
- Author preparation: Microsoft documentation advises GetDpiForWindow for per-monitor-aware calling threads; evaluate that supported path rather than mechanically applying GetDpiForMonitor despite its documented calling-context caveat. Sources: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow and https://learn.microsoft.com/en-us/windows/win32/api/shellscalingapi/nf-shellscalingapi-getdpiformonitor .
- Next: preserve candidate until the independent Sol/clean-code-guard report arrives, then batch fixes, author verification and a new frozen manifest before fresh independent rechecks. M1 remains unaccepted; M2 has not started.

## M1 / iteration 2 - fix batch / recheck round 1 of 3

- Author: `/root`. Sol iteration-1 ACCEPT and independent clean-code-guard PASS received for original manifest `76B1A9EC66BF2033B8DD095B51993A4120B398F8ECF229A25FE87914536EFA8B`; report `reviews/m1-iteration-1-sol.md`. Opus REVISE on the same original manifest controls the open gate. No author adjudication/waiver.
- Candidate-2 packet: `m1-candidate-2.md`. Current-DPI query uses a hidden primary-monitor PMv2 HWND and GetDpiForWindow; removed GetDpiForSystem. Reconciliation skips unchanged geometry/DPI. Test fixes cover explicit reference-thread DPI awareness, bounded input interference, interior pixels, exact failure exits, and configurable idle evidence paths. Tool-manager provenance and fullscreen overlap limitations documented.
- `M1-OPUS-MAJ-1`, `M1-OPUS-MIN-1..5`, `M1-SOL-MIN-1`: **fix-submitted**, not independently resolved. Fresh reviewer dispositions required. Independent guard on changed production code assigned to fresh Sol.
- Author checks: fmt, all-target warnings-denied Clippy, 3 unit tests, release build; final native smoke PASS 305 assertions after fixture correction. 10 warm-up plus 100 in-process cycles: every sample 11,055,104 private bytes / 245 handles / 7 USER / 0 GDI. No monotonic growth. New evidence under `artifacts/m1-candidate-2`; original evidence untouched.
- New binary SHA `4C5BC51B0EE71720A157997C6999235A5EDAD8F5D7787595B661BD10D2D4236B` matches smoke/idle. Idle: 30.006 seconds, private bytes 10,915,840 to 11,227,136, no CPU delta measurable by endpoint counter. Observation only; DWM/GPU/power and competitor comparison excluded.
- Frozen manifest: `reviews/m1-iteration-2.manifest`, **41 inputs**, SHA-256 `3080840DA84AE480AF07B382ABD4DF42D9E2E1956706EEAACB9693E3606E0D7C`. No M0 normative file changed. Progress remains intentionally outside the manifest. No ROQA process left running. M1 remains unaccepted and M2 has not started.

## M1 / iteration 2 - Opus ACCEPT received; Sol recheck pending

- Independent Opus disposition: **ACCEPT** for all 41 inputs of manifest `3080840DA84AE480AF07B382ABD4DF42D9E2E1956706EEAACB9693E3606E0D7C`. Report: `reviews/m1-iteration-2-opus.md`; task <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3Aa4cc43aa-1b53-4dd3-b847-37a150cf7241>.
- Within Opus's independent scope, `M1-OPUS-MAJ-1` is resolved by supported API/source review and message wiring; MIN-1..5 are resolved or documented. No real changed-since-sign-in scale transition was tested. Do not convert that limit into a physical test claim.
- Actual frozen harness, fresh-process run5: **PASS 305 assertions**, all captures stable on first attempt. Runs1/2 correctly INCONCLUSIVE under cursor interference. Runs3/4 invalid because of same-PowerShell-host fixture reuse; excluded from runtime acceptance evidence. Independent lifecycle samples flat at 8,056,832 private bytes / 239 handles / 7 USER / 0 GDI. A separate 200-broadcast diagnostic also had unchanged resources. Raw independent results are workload samples, not a cross-run memory guarantee.
- New nonblocking carry-forward findings: `M1-OPUS-R2-MIN-1` primary/topology DPI-message authority (M3 lifecycle work); `M1-OPUS-R2-MIN-2` static smoke anchor cannot be reused safely in the same PowerShell host (fix fixture before next runtime candidate; meanwhile fresh-process invocation only); `M1-OPUS-R2-MIN-3` physical DPI regression coverage remains manual.
- Pending manual checklist: when a real scale-change exercise is authorized, record sign-in/current primary scale, launch after changing scale without sign-out, verify both DIP sizes against current primary DPI, exercise settings/display reconciliation, restore prior settings and verify again. Also cover primary-monitor replacement and mixed DPI in M3. No system settings changed for this review.
- No candidate input changed following Opus ACCEPT. Sol same-manifest disposition and changed-code guard still required. M1 remains unaccepted and M2 has not started. Preserve frozen inputs until both reviews finish.

## M1 final disposition / M2 started

- Fresh Sol recheck: `reviews/m1-iteration-2-sol.md`, **ACCEPT** and independent clean-code-guard **PASS** for manifest `3080840DA84AE480AF07B382ABD4DF42D9E2E1956706EEAACB9693E3606E0D7C`. Task <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3Aa853defd-0eec-421f-869e-d0e281310f51>.
- **M1 accepted** by both independent reviewers on that identical manifest. Root reverified all 41 entries before starting M2. Initial DPI major and prior M1 findings resolved for the bounded milestone; physical changed-scale coverage remains unperformed, as both reviewers state.
- New nonblocking `M1-SOL-R2-MIN-1`: the author verification transcript omits native results; Sol independently captured passing checks. Future author check evidence must capture stdout/stderr with an explicit exit status, not rely on Start-Transcript alone.
- M2 will use bounded independently reviewed steps: first the real window-catalog/event/coalescing foundation (M2a); then interactive dock identity/icons/keyboard/UIA and text/IME probes (M2b). These are implementation candidates within the accepted M2 scope, not a weakening of M2 acceptance. Full M2 and stack go/no-go remain open until the complete contract passes both reviewers.

## M2a / candidate 1 - foundation implemented, author verification complete

- Author: `/root`. Packet: `m2a-candidate-1.md`. This candidate covers the catalog/event foundation only. M2b interaction/text proof and the cumulative M2 council stack gate remain pending; no M2 acceptance or M3 start claimed.
- Delivered: bounded 256-slot WinEvent notification buffer, 512-handle deduplication, 50 ms batching including follow-ups, one worker and capacity-one channels, targeted source updates, startup/overflow/30-second full repair, generation-safe publication without continuous-event starvation, provisional real executable/PID groups, filtered real title/state metadata and explicit worker/hook/timer teardown. No new launcher/UIA/media UI yet.
- Author caught and fixed an actual storm amplification bug before freeze: 501 changes initially caused 106 full scans; final observed burst caused 4 targeted batches, 4 HWND inspections and no full enumerations. This is an internal-workload observation, not a Nectar/Nusa comparison.
- Final captured fmt/Clippy/7 tests/build: PASS with explicit exit0 and stdout/stderr retained. Catalog native test PASS61; foreground/title/minimize/restore/show/hide/create/destroy/churn, periodic reconciliation, continuous publication and quit under continuing events exercised. Final lifecycle samples identical at 11,116,544 bytes / 248 handles / 15 USER / 0 GDI. Shutdown under storm169.1823ms. Unhook/join recorded before process exit.
- Final shell smoke PASS305 twice in the same PowerShell process. Fixture resets, thread join and class unregistration address the static fixture reuse defect; final focus baseline works without requiring Windows to grant a background test foreground rights. Earlier foreground-denial INCONCLUSIVE runs are not counted as passes.
- `M1-SOL-R2-MIN-1` and `M1-OPUS-R2-MIN-2`: fix-submitted in M2a; independent guard/review must judge the changed files. Physical DPI and topology-message authority remain manual/M3 follow-ups.
- Final binary SHA `78A4C04E562977EE53482E11FA30ACF927AA828184BFD69641FFAA549665224F`, consistent across all final evidence. Idle30.0114s: CPU15.625ms at endpoint-counter resolution; private bytes11,005,952to11,337,728. Observation only, no DWM/GPU/power/competitor verdict. No ROQA process intentionally left running.
- Known limits: provisional executable grouping; no packaged/elevated/hung-source runtime matrix, overflow/lost-event/failure injection, icons/activation/UIA/Narrator/text shaping/scaling/IME proof yet. Full M2 contract remains in force. Next freeze this candidate and obtain fresh non-author Sol+Opus initial reviews, including independent clean-code-guard.
- M2a frozen manifest: `reviews/m2a-iteration-1.manifest`, **61 inputs**, SHA-256 `C493F3615CBE4CEE643E3264D67C50961B09E44CD0C48A96E418A0A5BDAFBBAC`. All final binary identities/statuses and captured seven-test/build results checked before freeze. A local source/evidence snapshot is preserved at `artifacts/review-snapshots/m2a-iteration-1.zip` for future diffs; it is not a deployment or commit.
- Fresh independent initial reviews assigned to Sol High (code/architecture/clean-code-guard; no GUI launch) and Opus High (exclusive catalog/shell runtime slot). No author input may change during review. Both acceptances must cover this same manifest. Progress/review output remain excluded. M2a is not accepted yet; M2b and full M2 stack decision remain pending.

## M2a / independent Opus initial disposition

- Opus High: **ACCEPT for M2a foundation only**, manifest `C493F3615CBE4CEE643E3264D67C50961B09E44CD0C48A96E418A0A5BDAFBBAC`; report `reviews/m2a-iteration-1-opus.md`. Task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3A0773db2b-13ef-40ee-b6a6-b771f4181eca>.
- Independent evidence: all 61 entries match before/after; catalog PASS61; shell PASS305 twice in the same PowerShell host with visual capture inspection; fmt/Clippy/seven tests pass. The 501-title burst produced 5 targeted batches, 0 full scans and 7 inspections. Resource samples stayed flat at 11,141,120 bytes / 250 handles / 15 USER / 0 GDI. Quit during continuing events took 199.6 ms with unhook/join observed. Idle CPU remained below endpoint-counter resolution. These are local observations, not comparative performance claims.
- `M2A-OPUS-MIN-1`: open, fix before M2b interaction/modal loops. A lost DIRTY thread message can leave the coalescing latch set indefinitely; periodic repair must drain ingress/reset the latch or use robust window-message transport.
- `M2A-OPUS-MIN-2`: open, carry into next reliability candidate. Incomplete source snapshots need bounded retry/backoff to avoid repeated full scans under continuing events.
- `M2A-OPUS-MIN-3`: open/nonblocking. Opt-in diagnostic file failures currently terminate the app; transient-lock behavior needs an explicit policy.
- `M2A-OPUS-MIN-4`: open, carry into next reliability candidate. Add meaningful incremental-source/overflow/missed-change repair coverage.
- Candidate remains frozen pending completion of the independent Sol disposition/guard. The Sol report file now exists and contains a material WM_TIMER starvation finding; no gate acceptance or source mutation is inferred from an in-progress report. Full M2, M2b and stack go/no-go remain open; M3 cannot start.


## M2a / revision 2 in progress after independent REVISE

- Completed Sol High disposition: **REVISE**, report `reviews/m2a-iteration-1-sol.md`, original manifest `C493F3615CBE4CEE643E3264D67C50961B09E44CD0C48A96E418A0A5BDAFBBAC`. Task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3A32514462-b8d8-4e97-983d-b11f766f2958>.
- `M2A-SOL-MAJ-1`: fix-submitted by `/root`. Replaced catalog WM_TIMER scheduling with monotonic deadlines serviced on each bounded host-loop pass and a deadline-bounded wait. Neither sustained posted messages nor a lost COMPLETE wake can prevent collection/repair indefinitely. The stricter finding is addressed without author adjudication of the review split; both reviewers must judge the new manifest.
- `M2A-OPUS-MIN-1`: fix-submitted. Every outer-loop service drains callback ingress and resets the notification latch, independently of DIRTY delivery; idle missed wakes recover at the next finite repair deadline.
- `M2A-OPUS-MIN-2`: fix-submitted. Incomplete snapshots invalidate a full repair with 1/2/4/8/16/30-second retry backoff, capped at 30 seconds; continuing new events cannot bypass it; a complete result resets it.
- `M2A-OPUS-MIN-4`: partial coverage submitted. Deterministic tests cover actual catalog ingress overflow/lost latch, missed completion, single-flight follow-up batching, incomplete retry, full repair and disconnected-worker collection with no timer message. Source cache native failure/partial-incremental branches remain untested; do not claim complete source-side fault injection.
- `M2A-OPUS-MIN-3`: deferred/nonblocking. Explicit requested diagnostic writes remain fail-fast with an error; default launches have no diagnostic disk writes. No swallowing evidence failures or fabricated output. Transient-lock handling remains a documented limitation.
- `M2A-SOL-MIN-1`: deferred/nonblocking. Constructor decomposition is outside this behavior correction; ownership/partial-startup logic unchanged.
- Author check/build PASS with 11 unit tests and captured exits/streams. Native catalog PASS67: 3,495,344 successful queue posts and 70,655 capacity rejections during 36 seconds; metadata publication and periodic repair observed before the producer stopped. 501-title burst: 5 targeted batches, 5 inspections, zero full scans. Resource samples flat at 11,206,656 bytes / 248 handles / 14 USER / 0 GDI. Quit under continuing title events: 183.1402 ms. Author observations only; independent gate still open.
- Revision 2 author verification completed: shell PASS305 twice in one host and 30.0163-second idle observation (CPU below endpoint resolution; private bytes10,997,760 to11,198,464). All evidence matches binary `779D62B0567FA049CA80BAD9B771542B92849E0920F7592373230C8E94EF2988`.
- Revision 2 frozen: `reviews/m2a-iteration-2.manifest`, **71 entries**, SHA-256 `1E8AEE10CA8DC9C426B096B3AE122609E1BE032F639B55AEC1028A65B7BE2247`. Packet `m2a-candidate-2.md`; exact textual diff and API-contract evidence retained. Snapshot `artifacts/review-snapshots/m2a-iteration-2.zip` (164432 bytes). No normative PLAN change. Fresh independent Sol/Opus rechecks required; source/evidence frozen while they run. This is round 1 after the initial review and remains below PLAN's round cap.
- Fresh Sol High recheck/independent guard assigned: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3Acdb16488-541b-4336-8f48-b2b78b5ca991>. Fresh Opus5.5High runtime recheck assigned: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3A687211c1-8cbb-44cc-917b-3a1b80f138b4>. Both review identical revision2manifest; Opus owns native runtime slot, Sol offline only. Neither authored any reviewed input. No M2a acceptance until both dispositions arrive for this exacthash.


## M2a / revision 2 Opus independent ACCEPT; Sol pending

- Fresh Opus5.5High: **ACCEPT for M2a only**, exact manifest `1E8AEE10CA8DC9C426B096B3AE122609E1BE032F639B55AEC1028A65B7BE2247` (71 inputs). Report `reviews/m2a-iteration-2-opus.md`. Task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3A687211c1-8cbb-44cc-917b-3a1b80f138b4>.
- Independence/integrity: no candidate authorship/edits; all71 inputs verified before/after native runs; snapshot hashes and independently computed candidate diff match. Runtime binary unchanged at `779D62B0567FA049CA80BAD9B771542B92849E0920F7592373230C8E94EF2988`. Root reverified all71 after the disposition arrived.
- Independent runtime: catalog PASS67; queue producer posted3,119,383 messages, rejected51,700 at capacity; metadata and periodic repair progressed while producer ran. 501-title burst:6 targeted batches/0fullscans/373ms observed convergence. Resources flat. Quit under title storm213ms; zero-hooks stopped record. Shell PASS305 twice in one host; captures inspected, no interference/retries. Idle CPU below endpoint-counter resolution over30s. Scratch formatter/Clippy/11tests PASS. No guard claim by Opus; Sol owns independent guard.
- Opus independently resolves `M2A-SOL-MAJ-1`, `M2A-OPUS-MIN-1`, `M2A-OPUS-MIN-2`. `M2A-OPUS-MIN-4` partly addressed/open minor for source failure/native fault coverage; `M2A-OPUS-MIN-3` deferred minor; `M2A-SOL-MIN-1` remains for Sol disposition.
- M2b carry-forwards: publishing pauses while a modal loop owns the UI thread; use a modeless interaction route or integrate deadline servicing before introducing modal UI. A handle-less notification no longer refreshes foreground metadata immediately; next real event or bounded repair corrects it. Opus did not elevate this to a finding for M2a. Treat both as interaction design constraints, not full-M2 behavior acceptance.
- Limits preserved: queue not proven continuously full; no original-stall reproduction, real modal loss/native fault/hung/packaged/elevated/physical-DPI proof. M2b text/input/accessibility and cumulative go/no-go remain required beforeM3.
- **Gate remains pending Sol ACCEPT and independent clean-code-guard on the same frozen manifest.** No implementation/evidence changed and no M2b started while that review is outstanding.


## M2a accepted / M2b started

- Fresh Sol High **ACCEPT**, independent clean-code-guard completed: `reviews/m2a-iteration-2-sol.md`. Task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2d77151a-7f81-4668-9e28-acb5db4fdd45%3Adelegate-task%3Acdb16488-541b-4336-8f48-b2b78b5ca991>.
- **M2a accepted** by Sol and Opus against identical 71-input manifest `1E8AEE10CA8DC9C426B096B3AE122609E1BE032F639B55AEC1028A65B7BE2247`. Root reverified all71 before starting new work. Scheduling, wake recovery and retry findings resolved by independent dispositions. No blocker/major remains. Deferred minors: constructor decomposition, opt-in diagnostics transient-lock policy, remaining source/native failure coverage.
- M2b author: `/root`. First bounded candidate M2b1 proves a modeless standard-control window picker using the live shared catalog, explicit user-triggered open/activation, native filter edit/list/button keyboard routes and Windows accessibility proxies, Unicode/RTL rendering and system text-scale API integration. This is useful interaction and stack-risk evidence, not an icon-less claim of full dock parity. No mock app rows.
- M2b2 still owes real icons, packaged identity/grouping, wider activation/accessibility/failure matrix and any text/IME/Narrator/system-setting proof not established by the first candidate. Full M2 and the stack go/no-go remain open until the full accepted PLAN contract passes both reviewers. M3 cannot start. No normative PLAN changes or system configuration changes authorized by this ledger entry.


## M2b1 / candidate 1 - frozen, independent reviews being assigned

- Author: `/root` (new session continuing from thread `53d0a5f2-b6cf-4975-ac6e-b7fa1fead022`, which stopped on a usage limit, not a project problem). Packet: `m2b1-candidate-1.md`. Scope: modeless standard-control window picker on the live catalog (filter edit, list, buttons, keyboard routes, UIA through a native probe, Arabic/RTL and text-scale API integration). No icons/grouping (M2b2). Not a milestone acceptance.
- Author fixes this session: Clippy `collapsible_if` failure in `picker.rs` (first verification run **failed**, exit 1; fixed, logs overwritten by rerun); redundant `EnableWindow`; duplicate picker creation folded into `Desktop::open_picker`; harness failure handler no longer masks the real error and classifies foreground loss by owner.
- Author verification: `verify.ps1` fmt, all-target warnings-denied Clippy, 11 tests, release build all exit 0. Final binary SHA-256 `9D9F0A52DB71A18E306BDFA53AB5343F7B69405327E5932684A853048AEAC3A7`. Picker smoke runs 7 and 8: **PASS 217**; activation by Enter verified; 35 open/close cycles flat at 26 USER / 17 GDI, handles 347 to 350, about +135 KB private bytes. Catalog PASS67, shell PASS305, idle observation (0 ms CPU at endpoint resolution, 30.0078 s) on the same binary.
- Not clean: five other picker runs did not pass (two INCONCLUSIVE foreground denials, an old-harness focus-timeout FAIL, an old-harness passive-launch-foreground FAIL, one run whose failure handler masked its error). Foreground losses are **unattributed**; four other runs passed on the identical binary. Independent Opus asked to investigate. All kept under `artifacts/m2b1-candidate-1/`.
- Frozen manifest: `reviews/m2b1-iteration-1.manifest`, **117 inputs**, SHA-256 `22923E0D0F08A38CE05B2FCA45BE09DD9FAA2A8B3708269CB8F90B4F08755118`. Snapshot `artifacts/review-snapshots/m2b1-iteration-1.zip` (257806 bytes). Earlier WIP evidence `artifacts/m2b1-work/` is excluded and not final.
- Limits: Narrator speech, OS text-scale setting changes and real IME composition untested; picker-create failure is fail-fast; no elevated/packaged/hung target matrix. M2a deferred minors still open. M3 cannot start.
- Fresh independent reviews assigned on the single manifest above (neither authored any input): Sol High (`gpt-6.1-sol`, reasoningEffort high; offline, clean-code-guard, no GUI) <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Af9557832-b7d0-4221-8af7-992c6b340a76%3Adelegate-task%3A9a93d88b-24bd-4bbb-9197-25be6088f43e>; Opus 5.5 High (effort high; exclusive native slot, evidence under `artifacts/m2b1-opus-review/`) <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Af9557832-b7d0-4221-8af7-992c6b340a76%3Adelegate-task%3A2061c1d9-3de3-43ba-b62a-0beba98cd98b>. Candidate frozen while they run. No acceptance until both ACCEPT this exact hash; M2b2 and M3 not started.
- **Sol review did not run:** the delegated Sol High task failed immediately with "Your ChatGPT usage limit was reached" (no review output, no report file). No Sol disposition or clean-code-guard exists for manifest `22923E0D0F08A38CE05B2FCA45BE09DD9FAA2A8B3708269CB8F90B4F08755118`. No substitute reviewer was used and the author did not self-review. M2b1 cannot be accepted until a fresh Sol High task is run on this identical manifest once capacity returns. The Opus review continues independently; the candidate stays frozen.


## User process decision: M2b2 development before M2b1 acceptance

- 2026-11-09, user instruction: start building M2b2 now; council reviews to be conducted later "after we get things moving". Recorded as a user scope/process decision, not an author waiver. **M2b1 remains unaccepted** (Opus review running, Sol blocked on capacity) and M2b2 builds on unaccepted inputs at the user's direction; any later M2b1 findings must be merged forward.
- M2b2 work happens in a separate copy, so the frozen M2b1 inputs (manifest `22923E0D0F08A38CE05B2FCA45BE09DD9FAA2A8B3708269CB8F90B4F08755118`) stay byte-identical while Opus verifies them. No native GUI runs by the author while the Opus reviewer holds the live slot.
- M2b2 work in progress (author `/root`, working copy `../roqa-m2b2`, not frozen, not reviewed): dock now shows one slot per real catalog group plus a leading picker slot; grouping prefers the window's AppUserModelID (shell property store, MTA on the catalog worker) and falls back to the executable; icons come from a separate bounded STA worker (`IShellItemImageFactory`, AppsFolder AUMID then executable, 64-request queue, 256-entry cache, 1 s bounded shutdown) and are drawn with D2D; dock resizes in place (same HWND); click = taskbar-like activate / cycle / minimize with target revalidation; the picker remains the keyboard/UIA route. Offline author checks: fmt, warnings-denied Clippy, 16 unit tests, release build PASS; Cargo.lock unchanged. **No native runtime evidence yet** (waiting for the Opus reviewer to release the live slot). Dock icons have no UIA exposure yet.


## M2b1 / iteration 1 - independent Opus REVISE

- Opus 5.5 High: **REVISE** on manifest `22923E0D0F08A38CE05B2FCA45BE09DD9FAA2A8B3708269CB8F90B4F08755118` (all 117 entries and both binaries verified before/after). Report `reviews/m2b1-iteration-1-opus.md`; evidence `artifacts/m2b1-opus-review/`. Task <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Af9557832-b7d0-4221-8af7-992c6b340a76%3Adelegate-task%3A2061c1d9-3de3-43ba-b62a-0beba98cd98b>.
- Runtime: picker smoke 15 PASS / 1 INCONCLUSIVE; catalog 1 FAIL (harness precondition: producer never reached queue capacity, 0 rejections) then PASS67 twice; shell PASS305; checks exit 0; 210-cycle lifecycle flat after one +3 handle step. Arabic capture renders correctly.
- Author's unattributed foreground failures: no product race found; consistent with user input cancelling the test's foreground grant (0/10 with one injected keystroke). Not proven.
- `M2B1-OPUS-MAJ-1` (open): `--show-picker` never transfers foreground rights, so the keyboard route usually opens unfocused (2/10 without the harness grant); the denial status is then overwritten by the normal status. The smoke passed only because the harness granted foreground itself.
- `M2B1-OPUS-MIN-1` modal size/move and system-menu loops pause catalog/quit servicing (packet's "no modal loop" overstated); `MIN-2` harness misclassifies ROQA focus denial as interference; `MIN-3` restore failure in activation exits the host; `MIN-4` snapshot zip flattens paths (3 name collisions).
- Reviewer side effect reported: an Alt+Space probe opened PowerToys Run once; closed with Escape.
- Sol review still blocked on capacity. M2b1 not accepted. Per user direction, fixes are made forward in the M2b2 working copy and will be reviewed together.
- M2b2 WIP after Opus REVISE (author `/root`, working copy, not frozen): `MAJ-1` fix-submitted (`--show-picker` client passes its foreground right to the resident ROQA process only, via `AllowSetForegroundWindow(pid)`; denial status persists until the picker is focused); `MIN-3` fix-submitted (restore failure becomes a status message); `MIN-2` fix-submitted (picker smoke no longer grants foreground itself and treats the requesting app keeping focus as FAIL). `MIN-1` (modal size/move/system-menu loops) and `MIN-4` (zip paths, fix at next freeze) open. Checks/16 tests/build PASS.
- First native M2b2 runs: shell smoke INCONCLUSIVE twice (cursor instability: notch passed, dock geometry `16+56n` DIP, idle background and picker-glyph pixels passed before the hover capture); picker smoke INCONCLUSIVE (Windows denied initial fixture foreground). Desktop was in active use. A direct launch captured the dock with the picker glyph and five real app icons plus the active dot, then `--quit` exited cleanly (`artifacts/m2b2-work/dock-demo.png` in the working copy). No PASS yet for the new binary.
- M2b2 WIP step: dock UI Automation (author `/root`, working copy, unreviewed). `src/dock_uia.rs`: ToolBar fragment root on the dock HWND plus one Button per slot with InvokePattern, names from real window titles (+ window count, + "active"), live bounding rectangles from dock geometry, version-scoped runtime IDs (stale elements return UIA_E_ELEMENTNOTAVAILABLE), WM_GETOBJECT through a thread-local HWND registry, teardown via `UiaReturnRawElementProvider(NULL)` + `UiaDisconnectProvider` before HWND destruction. Providers read only an `Arc<Mutex>` snapshot and post `WM_APP+4` to the host loop. Dependency note: `windows-core =0.62.2` added as a direct dependency (already locked; Cargo.lock gains one dependency line, no new package) because `#[implement]` emits `::windows_core` paths.
- Author checks: fmt, warnings-denied Clippy, 16 tests, release build PASS. Live out-of-process UIA probe (author evidence, one run): ToolBar "ROQA Dock" with 8 Button children, names and InvokePattern correct, rectangles match slot geometry; Invoke slot 0 opened the picker unfocused with the persistent denial status; Invoke on an app slot was **refused foreground by Windows** (ROQA had no input) so the target did not activate. Real Narrator speech/invocation untested. The desktop was in active use (a full-screen game in the foreground); earlier smoke INCONCLUSIVE results are attributed to that, and author GUI runs are paused until the user reports the desktop free.
- M2b2 WIP: dock runtime test written, **not yet run** (desktop in use). `scripts/picker-smoke.ps1` gains a dock section: fixture group exposed as a named, invokable UIA button; real mouse clicks minimize the focused lone fixture, restore+activate it, and open a focused picker from slot 0. `tests/PickerProbe.cs` gains `Click` (SendInput) and `IsIconic`. Offline only: script parses, probe compiles.
- M2b2 WIP native author runs (working copy, binary SHA-256 `400FEEE6698A3D73735D32ED05229939A6F173A2AB2EF7A4C0DE3081EB10B5DC`, desktop reported free): picker+dock smoke `picker2` **FAIL** at the new dock section (harness bug: PowerShell 5.1 `ConvertFrom-Json` array wrapping; every picker check through the real `--show-picker` route had passed); fixed in the harness only; `picker3` **PASS 226** (UIA fixture button, click minimize, click restore+activate, slot-0 click opens a focused picker, keyboard route takes focus without a harness grant). Shell `shell3` **PASS 305** (dynamic dock geometry, background/glyph pixels, hover). Catalog `catalog1` **PASS 67**. Picker cycle samples: 18,190,336 -> 18,341,888 private bytes, handles 496 -> 500, USER 27, GDI 52 flat. Higher than M2b1 (about 12 MB / 347 handles / 17 GDI), consistent with shell icon extraction and loaded shell/UIA libraries; observation only, not attributed in detail.
- M2b2 WIP, user direction "use the system design on Nusa" -> user chose notch look + behaviour. Our own implementation (no Nusa code/assets; the Nusa page publishes no design tokens): black notch flush with the work-area top, bottom-only rounded corners, compact 200x32 DIP clock pill in the user's locale time format (DirectWrite, `GetTimeFormatEx`), hover expands in place to 360x84 DIP showing time over long date, pointer leave collapses. Redraws only when the minute changes or on hover: the minute deadline joins the catalog deadline in the blocking wait (no WM_TIMER, no polling). Skipped: idle "eyes" mascot, expand/collapse animation (reduced-motion handling belongs with M4), multi-monitor/fullscreen suppression (M3). Shell smoke updated (flush geometry, black background, hover proven by expansion and collapse).
- Author checks: fmt, Clippy, 17 tests, release build PASS (binary `29326A6008FCE92AA66D67AFEF099FD3900507801EA035C7E7F032273E6511B4`). Non-intrusive native check: notch 200x32 at work top, centered, focus kept, `--quit` exit 0. Full native suite **not run** on this binary (desktop in use); hover expansion has no runtime evidence yet.

## M2b2 work merged into the main tree; handoff to a new thread

- 2026-11-09, user instruction: move the work to a new thread and continue. Since Opus returned REVISE on M2b1 iteration 1, those frozen inputs can no longer be accepted unchanged; they stay preserved for audit in `reviews/m2b1-iteration-1.manifest` and `artifacts/review-snapshots/m2b1-iteration-1.zip`. `src/`, `scripts/`, `tests/`, `Cargo.toml` and `Cargo.lock` from the working copy `../roqa-m2b2` were copied into the main tree (src byte-identical to the copy); `artifacts/m2b2-work/` copied too. The working copy remains on disk (its `.tools` is a junction to this repo's `.tools`: remove the junction with `cmd /c rmdir` before deleting the folder).
- Main-tree verification: `verify.ps1` first attempt **failed** (exit 1, no JSON: its fixed 60-second step limit is shorter than the cold build with the new crate features); after a warm `check.ps1` and release build, `verify.ps1` PASS (checks exit 0, 17 tests, release build exit 0), evidence `artifacts/m2b2-work/verification-main/`. Main-tree binary `D00AB9C8E1C1E4B61FF233691826B3AE57C79E6152EB39183DBAC071C4D949C2` (differs from the copy's binary because build paths are embedded); all native runtime evidence so far is from the copy's binaries, so the next candidate needs fresh native runs on the main-tree binary.
- Nothing committed. M2b1/M2b2 not accepted; next candidate must combine the M2b1 fixes and M2b2 and get fresh Opus + Sol reviews on one manifest.

## M2b candidate 1 (combined M2b1 fixes + M2b2 dock + notch) - frozen, reviews held by user

- Author: `/root` (new thread continuing the handoff above). Packet: `m2b-candidate-1.md`. It supersedes M2b1 candidate 1, whose inputs (`22923E0D…755118`) stay preserved for audit only.
- Author fixes this session: `scripts/verify.ps1` gains `-StepTimeoutSeconds` (1-7200, default 900) instead of the fixed 60 s limit that failed the cold main-tree build; a step that overruns still fails. `M2B1-OPUS-MIN-1` decided: **accepted as a documented limit** (picker move/size/system-menu modal loops pause catalog, dock, clock, picker sync and IPC servicing until the loop ends; auto-reset IPC events are serviced afterwards). `MIN-4` fixed: the snapshot zip keeps manifest paths. Dispositions: `MAJ-1`, `MIN-2`, `MIN-3` fix-submitted (see packet).
- Author self-check on changed production code (not an approval): no code change required; observations SC-1 to SC-7 recorded in the packet for reviewers (host `Desktop` size/SRP, `FindWindowW`-based foreground grant trust, unverified AUMID lookup behaviour on hung targets, clock after sleep, silent dock refusal, button-up click, icon alpha heuristic).
- Verification on the main tree: `verify.ps1` PASS (checks exit 0, 17 tests, release build exit 0). Binary `D00AB9C8E1C1E4B61FF233691826B3AE57C79E6152EB39183DBAC071C4D949C2` (unchanged by this session's script-only edits).
- Native evidence on that binary (user confirmed the desktop free; one fresh process per script, each run once, none failed or inconclusive): picker+dock `picker-run1` **PASS 226** (keyboard route focused without a harness grant; UIA dock button + Invoke; click minimize/restore+activate; slot-0 picker focus); shell `shell-run1` **PASS 304** (first runtime evidence for the notch hover: 360x84 DIP in place, collapse on leave); catalog `catalog-run1` **PASS 67**; idle `idle-run1` 30.016 s, CPU 15.625 ms (one tick quantum, unattributed; earlier candidates 0 ms), private bytes 16,572,416 -> 16,662,528, handles 406 -> 411. Evidence `artifacts/m2b-candidate-1/`.
- Frozen manifest `reviews/m2b-iteration-1.manifest`, **81 inputs**, SHA-256 `0325C3D4A2B4BDA8C187A131FCE07ECA0995A56FE30967B23AFBD67A740BD70A`. Snapshot `artifacts/review-snapshots/m2b-iteration-1.zip`, SHA-256 `BABE2B6E75BEB90174E854A46ECECC43F02D7EBE582AD89719B743A9C3303A3B`, 305699 bytes; all 81 entries verified by hash at their manifest paths. Excluded: `progress.md`, WIP evidence (`artifacts/m2b2-work/`, `artifacts/m2b1-work/`).
- 2026-11-09, user decision: freeze only; **independent reviews held until the user asks**. No reviewer launched, no acceptance. When launched: fresh Sol High (offline, clean-code-guard) and Opus 5.5 High (exclusive native slot) on this identical manifest. M2b, full M2 and the stack go/no-go remain open; M3 cannot start.

## M2 continuation after candidate freeze: packaged apps, hung target, ghost-window fix (WIP, unfrozen)

- Reviews remain held by the user. Work continues in the main tree; candidate 1 stays preserved by its manifest and path-preserving zip. The source change below makes candidate 1's binary stale, so the next candidate needs a fresh full native suite and a new freeze.
- New evidence script `scripts/packaged-hung-check.ps1` (no input injection). It starts a real hung WinForms window (UI thread in kernel32 `Sleep`, `IsHungAppWindow` true), then ROQA, then Calculator and Settings (both `ApplicationFrameWindow` in one `ApplicationFrameHost`). It checks separate dock groups through UIA, non-blank icon pixels, a real `Ghost` window, one group for the hung app, and `--quit` with the hung window present. It closes only the app instances it started.
- Runs on the candidate-1 binary `D00AB9C8…`: run1 to run5 **failed or errored on harness bugs** (PowerShell `$null` -> "" in `FindWindow`; unquoted `Start-Process` arguments; a CLR STA wait that pumps messages so the fixture never hung; `,@()` array wrapping hid the frame windows). Run 5 already showed the dock appearing and the catalog publishing the hung window. **Run 6 PASS 14**: Calculator and Settings got separate dock groups (AUMID grouping proven on real packaged apps) with real icons; the catalog kept publishing with a hung window; `--quit` 146 ms. Run 6 also exposed a **product defect**: Windows' "(Not Responding)" ghost window appeared as a second, blank-icon dock group next to the hung app (the taskbar shows one).
- Fix (author `/root`, unreviewed): `window_source::inspect` skips top-level windows of class `Ghost` (`GetClassNameW`, which sends no message to the target). `verify.ps1` PASS (17 tests, release build), binary `04DA0E517BFBDFA3E674C0C60356A2BDA943CAC7534AC2864F7E4D96898C9F44`.
- Runs on `04DA0E51…`: run7 PASS (one hung-app group; no proof that a ghost existed yet). The script then gained a precondition that a real `Ghost` window exists. Run8 PASS 16 and **run9 PASS 16** (final script SHA-256 `CC86F13E…0A8A`): ghost present, one hung-app group, Calculator and Settings separate with icons, `--quit` 131 ms. Evidence `artifacts/m2-packaged-hung/`.
- Still owed for M2: elevated-window behaviour (expected: listed and activatable, but `ShowWindowAsync` minimize blocked by UIPI; unverified), Narrator core flow, an OS text-scale setting change, real Arabic/IME typing into the picker edit (all need the user), the full native suite on the new binary, a new frozen candidate, the reviews, and the council stack go/no-go.

## M2 user-assisted session (binary `04DA0E51…`, WIP evidence, unfrozen)

- 2026-11-09, user present. Helper `artifacts/m2-assisted/assist.ps1`; log `artifacts/m2-assisted/assisted.jsonl` (each line has the binary hash).
- **Elevated window:** the user opened Task Manager. Token integrity read 0x3000 (High) against 0x2000 (Medium) for ROQA/test shell. (The helper's first rough probe, `Get-Process().Handle`, wrongly suggested non-elevated; superseded by the token read.) The dock listed "Task Manager" with its icon. An injected click on its slot **activated** it (foreground moved from T3 Code to Task Manager). A second injected click was impossible: Windows blocks `SendInput` from Medium while a High window is foreground (harness limit). The user's **physical click: nothing happened**. Control from a Medium shell: `ShowWindowAsync(SW_MINIMIZE)` on the same window returned false, `GetLastError` 5 (access denied), not iconic. Documented behaviour: elevated windows are listed and activatable; **minimize is refused by Windows (UIPI) and ROQA gives no feedback** (dock has no status line, SC-5).
- **Narrator (user report):** picker flow works. Clicking "All windows" opened the picker and Narrator announced the filter edit; typing, Tab, arrowing read list items, and Enter activated the chosen window. **Dock hover was silent.** Diagnosis: managed UIA `AutomationElement.FromPoint` at slots 0-2 returned the correct named Buttons (root ToolBar "ROQA Dock", control/content element, on screen), and the user's Narrator setting "read and interact with the screen using the mouse" is off (`InteractionMouse=0`, read-only registry check). So the silence is explained by that setting, but **Narrator reading the dock is unverified** (the user skipped a retest with the setting on). The dock isn't keyboard-focusable by design; the picker is the keyboard/Narrator route.
- **OS text-scale change: not established.** The user reported 125%, but the registry value, a fresh `UISettings.TextScaleFactor` (1.0) and a Settings capture (slider at minimum, Apply disabled; cropped, the full capture deleted for showing the account name/email) all showed 100%. The picker font matched the reported factor (-15 = -15 x 1.0 at 96 DPI). The user skipped the retry. Live relayout on a real setting change remains **untested**. Unusable captures (picker covered by another window) were deleted.
- Arabic keyboard typing: skipped by the user. The Arabic evidence remains UIA ValuePattern injection plus rendering (picker smoke), not keyboard/IME input.
- ROQA `--quit` exit 0, dock gone. No system setting was changed by the author.

## M2b candidate 2 - frozen, reviews held by user

- Author `/root`. Packet `m2b-candidate-2.md`. Supersedes candidate 1 (`0325C3D4…0BD70A`, never reviewed, preserved). Delta: ghost-window catalog fix (`src/window_source.rs`), `scripts/packaged-hung-check.ps1`, README dock/notch section (it replaces candidate 1's stale "no icons or grouping yet" text), plus the packaged/hung/elevated and assisted Narrator evidence recorded above.
- Binary `04DA0E517BFBDFA3E674C0C60356A2BDA943CAC7534AC2864F7E4D96898C9F44`, probe `07A86EE5…485D`. `verify.ps1` PASS (17 tests). Full native suite after the user confirmed the desktop free, each run once, none failed: picker+dock **PASS 226**, shell/notch **PASS 304**, catalog **PASS 67**, packaged/hung **PASS 16** (`--quit` 125 ms), idle 30.016 s with CPU 46.875 ms (3 ticks; higher than candidate 1's 15.625 ms on a busier desktop, not attributed). Evidence `artifacts/m2b-candidate-2/`.
- Frozen manifest `reviews/m2b-iteration-2.manifest`, **115 inputs**, SHA-256 `D6056EBEE3C87149A5F00FBB4154E647FE026DEC49B3E0E3929A2446A04D970E`. Snapshot `artifacts/review-snapshots/m2b-iteration-2.zip`, SHA-256 `9FCE5E550DB7B7629C4ABDF606C24F835B2C45D617327E8DB922C973BDD230E9`, 486221 bytes; all 115 entries verified by hash at their paths. It includes `artifacts/m2-packaged-hung/` (with the failed harness runs) and `artifacts/m2-assisted/`.
- Reviews still held per the user's decision; no acceptance. Still owed before the full-M2 gate: reviews of this manifest, Narrator reading the dock, an OS text-scale change with live relayout, real keyboard/IME text input, then the council stack go/no-go. M3 cannot start.

## Design pass 1: Nusa-like notch and dock (main tree, unfrozen; candidate 3 will include it)

- 2026-11-09, user request: "Work on the design: it should look and behave like Nusa." The UI is judged by three independent models: Claude Opus 5.5 High, Gemini 3.8 Flash High and Sol 6.1 Medium. Author `/root`. Design work continues in the main tree, so the main tree now differs from candidate 2. Candidate 2 stays preserved by its manifest and path-preserving zip. Its council reviews remain held by the user. The next frozen candidate (candidate 3) will include this pass.
- **User scope decisions (2026-11-09):**
  - Animation lands now, rather than in M4 as the PLAN had it, because the user asked for Nusa-like motion explicitly.
  - Live Nusa may be captured as comparison evidence. Captures stay under ignored `artifacts/` and are never shipped.
  - The expanded notch is a 320×80 time/date glance.
  - The dock gets Nusa's material plus a hover lift, with no magnification.
  - **ROQA always animates, like Nusa, and ignores Windows "Animation effects".** That setting is off on this machine (`SPI_GETCLIENTAREAANIMATION` false, read-only check). The first build honoured it and snapped (capture `design-review-1/roqa/`). After a recommendation to honour the setting with an opt-in flag, the user chose "always animate". The reduced-motion path was removed. This is a known accessibility trade-off against the original brief and stays recorded.
- **Docs:** `docs/design/nusa-reference.md` (public sources plus live-capture measurements), `docs/design/roqa-visual-spec.md` (tokens, motion, limits) and `docs/design/review-log.md` (judging rounds).
- **Implementation** (no Nusa code or assets; Nusa binaries and resources not opened):
  - The notch is a fixed 368×124 DIP HWND. Its window region is the collapsed footprint at rest and the whole window otherwise.
  - It has an anti-aliased Direct2D outline with concave ears, a Direct2D shadow effect while open, and Segoe UI Variable text with a smaller am/pm.
  - Hover intent: a 320 ms swell, then a damped-spring open (≈7 % overshoot) and a spring close (≈9 % undershoot). These are fitted to live Nusa frames.
  - The dock is a black anti-aliased bar with a hairline, hover fill, whole-pixel icon lift and an active pill. Its geometry, hit-testing and UIA are unchanged.
  - Frames are paced by `DCompositionWaitForCompositorClock` only while motion runs; idle returns to the original blocking wait.
  - New `src/motion.rs`; pinned `windows-numerics =0.3.1` (already in the lockfile) for Direct2D points.
  - `scripts/smoke.ps1` now asserts the region box, the open/closed state and the new colours.
  - `scripts/measure-idle.ps1` gains `-Executable` for before/after idle runs; the baseline binary is saved at `artifacts/design-baseline/roqa.exe` (`04DA0E51…`).
- **Author verification:** `verify.ps1` PASS for each build (`artifacts/design-review-1/verification*`); the judged binary `5F70A584…` has 20 tests.
- **GUI capture runs** (the user confirmed the desktop free before each): Nusa run 1 and run 2; ROQA run 1 (`8A41DF77…`, snapped under reduced motion: an *inconclusive* motion capture, kept); run 2 (`20351A90…`); run 3 (`5F70A584…`, the judged evidence). Every ROQA run quit gracefully with exit 0.
- Round-1 judges launched on manifest `artifacts/design-review-1/inputs.sha256` (SHA-256 `A69E033E…3BFE`): the [Opus](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3A680a425a-dc0a-48ee-ac8d-7418cdb944d1>), [Gemini](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3A6be782d1-b99f-4313-9e1d-3b7fba6602b5>) and [Sol](<t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A2ae59e2e-c401-47f3-8594-6d52ad9b9a0f%3Adelegate-task%3A3f4aebc0-4028-4e24-919a-320bfbabdf2c>) judges. Results are in `docs/design/review-log.md`.
- **Not yet run on the new binary:** the full native suite and the before/after idle measurement. The independent clean-code-guard review is pending too. M2 status is unchanged: nothing accepted.

## Design continuation: Claude deferred, shadow input fix submitted

- User direction 2026-11-09: skip Claude judge for now and continue. Failed round-2 Claude task cancellation requested; no replacement. Held M2 council reviews and candidate-3 freeze remain held.
- Recovered independent round-2 results: Gemini POLISHED; Sol REVISE, with only the shadow input interception major unresolved. Full reports saved as docs/design/review-round-2-{gemini,sol}.md; dispositions in review-log.md. No stale judgment applied to the new binary.
- Author /root (Codex continuation) split the notch shadow into a separate layered, transparent, nonactivating HWND, with scoped Canvas/window ownership and matching placement. Body input region excludes the shadow band. No copied assets, hooks, settings changes or taskbar changes.
- Author verification PASS (21 tests and release build), binary 045273D09564544BD917647B3243F13135DE7BE838CB6896E3222CE4ADEC100B. Evidence artifacts/design-review-3/verification. Shell-run1 invalid wrapper invocation after BOM corruption; all outputs preserved with warning. BOM fixed; shell-run2 PASS including shadow WindowFromPoint checks and lifecycle/resource checks. Each GUI run had fresh user confirmation and foreground check (T3 Code).
- Independent Sol High clean-code-guard reviewer: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3A58e4679e-c89b-4a1c-9e6b-dc44ea2ef3a2>. Exact current inputs artifacts/design-review-3/inputs.sha256. Pending; author cannot approve.
- Still required: actual underlying-process click evidence during open/close, fresh visual evidence and Sol/Gemini judgments, remaining native suite and matched before/after idle. Non-1.0 text scale and prior manual M2 limits remain open. Nothing committed or accepted.

## Silhouette and clock-status fix submitted for fresh review

- Independent clean-code-guard review returned REVISE on prior manifest BA21D011…FBB1: CODE-SOL-001 major (rectangular ear band still intercepts input), CODE-SOL-002 minor (occlusion/unexpected compositor-clock statuses skip fallback). Full report docs/design/code-review-3.md.
- Author /root fixes: analytic one-pixel silhouette rows for the GDI window region and matching circular-edge hover slop; shared D2D radius clamp; drawing and region use identical frame progress without speculative lead. Temporary GDI regions clean up on every error and only successful SetWindowRgn transfers ownership. Clock return classification now explicitly preserves IPC event/clock tick/timeout and uses bounded message wait for other statuses, following Microsoft function reference plus compositor-clock example. These are fix-submitted, not independently resolved.
- New binary 21BBFE1A70CB87AAFE48268AC75469B6335BD566FEEE2A51851703BB8ED428C6. verify PASS, 22 tests; shell PASS325 (actual underlying-process click delivery in rest corner, opening side shadow, open side shadow/corner and closing side shadow/corner), picker/dock PASS226, catalog PASS67, packaged/hung PASS16, quit144ms. Fresh capture active-fixture foreground true, graceful quit0. Each GUI run separately confirmed; T3 Code foreground on preflight. Evidence artifacts/design-review-4.
- Consecutive 30-second idle samples: baseline and current both CPU delta0ms at process-counter resolution. Baseline private bytes22982656->23199744, handles417->422; current20103168->20303872, handles445->450. Single observations only, workload/counter variance, differences not attributed; no DWM/GPU/power or animation frame-time evidence and no performance advantage claimed.
- Exact WIP recheck inputs: artifacts/design-review-4/inputs.sha256, 609 entries, SHA256 15B6A13CB06C16E37D7112C02751DAA57A594CF5FEA00A491CFE61CE0FA4F67F. Preserved paths in inputs.zip, SHA256 6C22AE9FC9E19F5EF2EBF47A8B244BDCE43F23E8DCF872115EC10E39FB5CCA00. Fresh Sol Medium and Gemini High design judges, plus fresh Sol High clean-code-guard recheck launched; task links in docs/design/review-log.md. All pending. Claude deferred by user. M2 candidate-3 freeze/council remain held, manual M2 limits remain open, no acceptance or commit.

## Round 3 Gemini completion (2026-11-09)

Gemini 3.8 Flash High returned **POLISHED**, independently verifying all 609 entries of manifest `15B6A13CB06C16E37D7112C02751DAA57A594CF5FEA00A491CFE61CE0FA4F67F`. No new scoped findings. It resolved the silhouette/input and compositor-status findings and accepted the existing minor allocation/dock-corner deferrals. Disposition record: `docs/design/review-round-3-gemini.md`; full report: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3Aab09d185-064d-4241-b2f8-b13c9bc06ad4>.

Sol Medium design and Sol High clean-code-guard results remain pending. Claude remains deferred. This is a WIP review, not candidate-3 milestone acceptance. No source/evidence input was changed for this record; candidate freeze and council remain held. Hardware text scale, Windows 10 execution, physical Narrator/Arabic/IME and compositor/performance limits remain open. A 0-ms counter sample is not proof of zero cost.
## Round 3 remaining results and expansion-probe correction (2026-11-09)

Sol Medium design returned REVISE on the exact 609-entry manifest 15B6A13C...4F67F: DESIGN-SOL-R3-001 major validation gap. The opening-labeled click at 120 ms occurs during 320 ms hover intent; expansion pass-through was not established. No visual redesign or new production failure demonstrated. Report record: docs/design/review-round-3-sol.md; full source thread linked there.

Independent Sol High clean-code-guard returned ACCEPT code-only on the same inputs, 0 findings, CODE-SOL-001/002 resolved. Report record: docs/design/code-review-4.md; full source thread linked there. This does not close the runtime evidence gap or grant milestone acceptance.

Author /root corrected scripts/smoke.ps1 only: retained and accurately renamed hover-intent case; added a geometry-synchronized expanding-side-shadow case at observed region height 48–64 DIP, with real fixture click and post-injection height below 76 DIP and not shrinking. Records region bounds before/after, local click coordinates, observation timing and fixture receipt. Production binary unchanged. Script parse passes; corrected native run is pending fresh desktop-free confirmation. Old review-4 manifest/archive/evidence remain preserved; its script disk entry is now superseded. Fresh scoped recheck required after new evidence. Claude and candidate-3 freeze/council remain held; nothing committed or accepted.
## Expansion-phase evidence submitted (2026-11-09)

Fresh user confirmation and foreground preflight (T3 Code Nightly PID20204) preceded corrected shell-run1 in artifacts/design-review-5. PASS329, all seven real fixture clicks received. Expansion observed region82,0,236,48 ->72,0,256,54, click80,24; 374ms to observed expansion and 8ms observation/injection bracket. Intent remained33px high; open80px; closing70->67 and67->64. No production code/binary changed. Against prior manifest, only scripts/smoke.ps1 changed; old archive/evidence preserved. Full suite/visual/idle evidence remains on identical production inputs; no unnecessary GUI reruns.

Fresh WIP exact manifest: artifacts/design-review-5/inputs.sha256, 622 entries, SHA256 905E1E912620A709FDBFB89D632ADF4FB1AB32F6879C6E0A5074D7806265493C. Path-preserving ZIP SHA256 2683A0ADC6616F0167A207D2EB786DD50991E9734BF257657DE9791E7C587E69. README/brief explain the superseded opening evidence and scope.

Fresh scoped Sol Medium review: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3A3501030e-7f16-431e-9969-45d2b301aefe> (pending).
Fresh scoped Gemini High review: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Adffe4a84-2175-47b0-b8b1-c818412673a3%3Adelegate-task%3A40697107-7c73-40f5-9b75-2aa6371f579d> (pending).

Independent code-only ACCEPT retained for unchanged production code; expansion finding remains fix-submitted pending independent recheck. Claude deferred, M2 council/candidate-3 freeze held, no acceptance/commit. Manual and performance limits unchanged.
## User acceptance and commit authorization (2026-11-09)

User explicitly instructed: "accept and commit" after the corrected PASS329 shell run and launch of the scoped evidence rechecks. Record this as user acceptance of the current design pass and authorization to commit its implementation, harness and review records. It is not an independent reviewer verdict: latest Sol/Gemini expansion-evidence rechecks are still pending at commit preparation. Claude remains deferred. Independent clean-code-guard ACCEPT applies to unchanged production code.

All 622 review-5 inputs reverified against the manifest immediately before commit preparation; git diff --check passed. No additional GUI run or production edit. Existing full-suite results and hardware/accessibility/performance limits remain as documented. M2 candidate-3 freeze and milestone council are not performed or accepted by this design commit. Unrelated user .gitignore edit is excluded. Ignored binaries, captures and review archives remain local.
## Sol expansion-evidence recheck complete (2026-11-09)

Fresh independent Sol Medium returned POLISHED on exact review-5 manifest905E1E91...5493C, 622/622 disk/archive entries verified. DESIGN-SOL-R3-001 resolved: increasing intermediate geometry brackets real click delivery to the separate-process fixture; prior silhouette/input findings resolved within sampled scope. No new findings. Full report thread and evidence/limits recorded in docs/design/review-round-4-sol.md. Production code unchanged since commit1d9e058; user acceptance already recorded. Gemini recheck remains pending; Claude deferred; milestone council/freeze and manual/performance limits unchanged. Review result committed as documentation under the user's commit authorization.
## Follow-on thread requested (2026-11-09)

User instruction: "after everything is done, start a new thread with the next steps, skip the counsel for now". After final Gemini design recheck is recorded and current design wrap-up completed, launch a new top-level ROQA thread (same project/root checkout) with the remaining M2 interaction/accessibility validation and subsequent implementation steps. Skip milestone council reviews for now per latest explicit user direction; do not invent council acceptance. Claude remains deferred. Do not repeat finished design/native work without cause. Carry forward manual evidence gaps, always-animate choice, independent clean-code-guard requirement for new nontrivial production edits, native taskbar/no-system-mutation constraints and fresh desktop permission per native run. Leave unrelated .gitignore edit untouched. The new thread is explicitly user-requested, not a substitute review delegate.
## Design pass complete; next thread authorized (2026-11-09)

Final Gemini High scoped recheck POLISHED on review-5 manifest905E1E91...5493C; all622 disk/archive entries verified, DESIGN-SOL-R3-001 resolved, no new findings. Report docs/design/review-round-4-gemini.md includes full source thread link and notes the original report's width-arithmetic typo without changing raw evidence. Sol Medium POLISHED and independent clean-code-guard ACCEPT already recorded. Current design pass is complete and user-accepted; implementation1d9e058, Sol report1522a7f, final report committed separately. Claude remains deferred. No unmeasured performance or outstanding manual/platform evidence is certified.

Per latest user instruction, skip milestone council for now and launch a new top-level thread on the same ROQA checkout for remaining M2 validation and next implementation steps. This explicit user direction supersedes the council-gating workflow while deferred; it does not manufacture council approval or mark untested gates passed. Start by assessing existing M2 evidence gaps (Narrator/keyboard, physical Arabic/RTL/IME and live text scale), prepare safe bounded validation, then continue the plan as warranted. Obtain fresh desktop-free confirmation before any native run. No system settings changes without authorization. Keep native taskbar, pinned tooling, no copied assets, independent clean-code-guard for nontrivial new production code, and unrelated .gitignore edit untouched. No further design-review delegates required.

## Follow-on M2 validation preparation (2026-11-09)

Author: primary Codex agent (gpt-6.1-sol, medium), user-requested follow-on thread. Audited rules, plan, product, ledger and picker/input/text-size source and smoke coverage offline. HEAD ae6693645bf0434898d91784a2d41323e8246366; release binary SHA-256 verified as 21BBFE1A70CB87AAFE48268AC75469B6335BD566FEEE2A51851703BB8ED428C6. No rebuild, native execution/input, setting change, production edit, freeze or commit.

Prepared docs/development/m2-assisted-validation.md with current-binary Narrator/keyboard failure recovery, physical Arabic versus actual IME composition, separately authorized live text-size/restore, desktop/foreground safety, result categories and M3 sequencing. Prior assisted speech belongs to the earlier binary; automated UIA and source subscriptions are not physical input/speech or live text-scale passes.

Council remains skipped; no reviewers or replacement judges requested. All new manual cases NOT RUN pending fresh desktop-free confirmation and user-operated prerequisites. M2 is not newly accepted. M3 implementation waits for required runtime evidence or explicit user scope decision for unavailable prerequisites, not council tasks. Fresh independent non-author clean-code-guard remains required for nontrivial production fixes. Existing .gitignore edit and prior evidence remain untouched.

## Current-binary assisted session started (2026-11-10 local)

User confirmed desktop free for keyboard/Narrator session. Read-only foreground check found T3 Code foreground, no existing ROQA and Narrator not running. Existing release hash matched 21BBFE1A70CB87AAFE48268AC75469B6335BD566FEEE2A51851703BB8ED428C6. Computer Use launch reported no targetable window; read-only native reconciliation confirmed one new owned resident PID 28676 at expected path, dock HWND 1313430 and notch HWND 1772622, with T3 Code still foreground. No repeated launch or injected input. Observation 2026-11-09T22:00:15Z is November 10 in Africa/Cairo. Record artifacts/m2-assisted-next/20261110-0000/session.json.

Resident intentionally retained for active user-assisted flow. Narrator/physical keyboard/failure cases still NOT RUN pending user operation; no settings changed, no new acceptance or council task. Fresh desktop confirmation is required for a separate later native run. Cleanup is pending completion of this session, not claimed complete.

## Assisted-session scope update (2026-11-10 local)

User explicitly skipped Narrator: current-binary speech check NOT RUN by user choice, not passed. User agreed to physical Arabic/RTL testing with an available layout. Read-only check found the same owned ROQA PID 28676, T3 Code foreground and no picker open. Current window inventory provided no Arabic-titled target; first user-driven subcase is physical filter entry/caret editing only, not matching-title activation or IME composition. No automated typing, setting changes or additional native launch. Awaiting physical user observation; cleanup remains pending active session completion.

## Physical Arabic entry/editing user report (2026-11-10 local)

Asked the user to open All windows, switch to the available Arabic layout, physically type the test word مرحبا (not paste), test arrow keys/Backspace and leave the picker open. User answered yes to correctly joined display and normal caret movement/deletion. Scoped result: PASS by user report for physical Arabic filter entry, joining and basic editing on the active session. No screenshot, codepoint readback or independent instrumented verification was collected for this subcase; layout identity and exact final edit contents were not measured. This is not mixed-title filtering/activation, full RTL UI validation, IME composition or Narrator evidence. Those scopes remain untested/skipped as previously recorded. No settings change or new native run; resident cleanup remains pending session completion.

## Accessibility checks deferred by user; session cleaned up (2026-11-10 local)

Latest explicit direction: skip the accessibility checks. Stop requesting Narrator, IME, additional Arabic/RTL and live text-size checks for now. Preserve the scoped Arabic user report; all omitted cases remain NOT RUN/deferred, not passed. No text-size setting was changed. Council remains deferred with no replacement judges; no M2 acceptance is manufactured.

Closed only the owned test resident after validating PID 28676, executable path and recorded creation time. Read-only foreground check found T3 Code. Hidden --quit client exited 0 and the owned resident exited within the bounded wait; no forced termination, injected input or system mutation. Active assisted session is complete. Earlier session.json is a starting observation; this entry supersedes its pending-cleanup status.

Prepared docs/development/m3-topology-preparation.md from current host/native-window/layout coupling: deterministic monitor model first, verified platform contracts, monitor-keyed surface reconciliation and targeted tests before native hardware validation; fullscreen/previews/recovery are later bounded slices. This is preparation, not implementation or acceptance. Existing .gitignore edit, frozen inputs and raw evidence remain untouched. Fresh non-author clean-code-guard still applies to future nontrivial production changes.

## M3 topology candidate 1 implemented (2026-11-10 local)

- **Author and scope.** Author: Claude Opus 5.5 High, primary agent in the user-requested top-level implementation thread. The user skipped the milestone council and accessibility checks; both are deferred and NOT RUN, not passed. Packet: `docs/development/m3-topology-candidate-1.md`. Base HEAD `ae6693645bf0434898d91784a2d41323e8246366`. Uncommitted, not frozen.
- **Change.**
  - New `src/topology.rs`: a monitor snapshot model with settle/diff and 5 tests, covering negative origins, work-area/DPI changes, reorder, removal/addition, and unchanged and transient snapshots.
  - `native_window::monitors()`: bounded `EnumDisplayMonitors` + `MONITORINFOEXW` enumeration, with DPI read through a per-monitor probe HWND.
  - `src/host.rs`: notch and dock surfaces are keyed by monitor device plus kind, with per-surface motion/hover/UIA state. Topology reads are coalesced and retried on a bounded schedule. The catalog/worker and IconService stay shared, and the picker stays on the primary monitor.
- **Verification.** `verify.ps1` PASS: 27 tests plus the release build. Binary `A822BF9CAE160F37B52109A15A1DFA465BFFAA751FA560A782FC952FC7CBB13B` replaces `21BBFE1A…C6` in `target/release`. Evidence: `artifacts/m3-topology-1/`.
- **Clean-code-guard.** Independent non-author reviews:
  - First review on `0F223E15…050294`: REVISE, with M3T1-CCG-MAJ-1 plus MIN-1, MIN-2 and MIN-3.
  - Author fixed MAJ-1, MIN-1 and MIN-2 and deferred MIN-3.
  - Fresh recheck on `inputs-2.sha256` `1E698C0FF0CBD2CE05B45282D1965F78B17BEAA9B85A0F922063D5D5F86E0BC1`: **ACCEPT, code-only**. MAJ-1, MIN-1 and MIN-2 resolved, the MIN-3 deferral accepted, no new findings.
  - This is not council or milestone acceptance.
- **No native run in this thread.** This machine has two monitors: DISPLAY1 is primary, and DISPLAY2 sits at x=-1920 (read-only query). The existing title-based native suites assume a single notch and a single dock, so they need a multi-monitor-aware update before they count as evidence. Hardware placement, hotplug, DPI/work-area change, cleanup and per-monitor cost all remain open. Each native run needs fresh desktop-free confirmation.
- **Untouched.** Prior M2/accessibility/Windows 10/compositor limits are unchanged. The `.gitignore` edit and earlier evidence are untouched.

## M3 topology candidate 1: passive two-monitor run (2026-11-10 local)

- **Consent and preflight.** The user said "go for it" in reply to the request for a desktop-free run. This was taken as confirmation for one run. Read-only preflight just before launch:
  - T3 Code (Nightly) PID 11868 was in the foreground;
  - there was no ROQA process or surface;
  - the binary matched the reviewed `A822BF9C…B13B`;
  - the monitors were `\\.\DISPLAY1` (primary, 0,0 1920x1080) and `\\.\DISPLAY2` (-1920,0 1920x1080). On both, the work area equals the monitor area at 96 DPI.
- **Script.** New `scripts/topology-check.ps1`, a passive harness. It launches the build once, enumerates surfaces by class and PID, checks each monitor's surfaces against that monitor, takes cropped captures, then runs `--quit`. It injects no input and moves neither the cursor nor the foreground.
- **Result: PASS, 25 checks.**
  - Each monitor has exactly one notch, one shadow and one dock.
  - On each monitor, the notch (400x124) is centred and flush with the top, the shadow matches the notch, and the dock (64 px high) is centred and sits 12 px above the bottom of the work area. That includes DISPLAY2, where the notch is at -1160,0 and the dock at -1080,1004.
  - The foreground was unchanged after launch and after quit.
  - `--quit` returned client exit 0, and the host exited with code 0 in 150 ms. No surface remained.
  - The captures show both notches drawing the time and both docks drawing icons.
  - One resource sample, with 2 monitors: private bytes 17543168, handles 451, GDI 42, USER 21. This is an observation, not a per-monitor cost measurement or comparison.
  - Evidence: `artifacts/m3-topology-1/native-1/`. The captures also contain parts of the T3 Code window behind the notch/dock crops.
- **Not covered.** Hotplug, reorder, DPI/work-area changes, mixed DPI, failure injection, per-monitor cost deltas and the multi-monitor update of the older title-based suites. The legacy smoke `DpiChange` case posts a synthetic `WM_DPICHANGED`. The host now re-reads the real DPI and ignores that value, so the case needs to be rewritten rather than rerun.
- **Status.** No settings changed. No council review, acceptance or commit.

## Native suites made multi-monitor aware (2026-11-10 local)

- **Request and author.** The user asked to "update them and commit the changes". Author: Claude Opus 5.5 High.
- **Lookups.** `tests/NativeProbe.cs` adds `Surfaces(title)`, which enumerates by class and title on every monitor, and `PrimarySurface(title)`. `smoke.ps1`, `picker-smoke.ps1`, `catalog-smoke.ps1` and `packaged-hung-check.ps1` no longer use `FindWindow`. They follow the primary-monitor surface, so their existing primary-geometry assertions keep their meaning. Checks for no pre-existing surface, no orphan and a hidden surface now require absence on every monitor.
- **DPI case.** The smoke `DpiChange` case now asserts the M3 contract: a synthetic `WM_DPICHANGED` with no real DPI change keeps the same notch HWND at the real DPI. Previously it asserted a rebuild at the fake DPI.
- **Verification.** `NativeProbe.cs` compiles, all five scripts parse with 0 errors, and a read-only probe returned 0 surfaces. **The updated suites have NOT been run.** Their results on the M3 binary are unknown until a separately confirmed desktop-free run. The prior PASS counts (shell 329, picker/dock 226, catalog 67, packaged/hung 16) belong to the earlier binary.
- **Commit.** At the user's request, this commit contains the M3 topology candidate 1 code, the scripts and the docs. The user's `.gitignore` edit is excluded and ignored artifacts stay local. This is not council or milestone acceptance.

## M3 fullscreen suppression candidate 1 (2026-11-10 local)

- **Request and scope.** The user said "wait on tests and go for the next feature". Native suites stay NOT RUN on purpose. Author: Claude Opus 5.5 High. Packet: `docs/development/m3-fullscreen-candidate-1.md`. Base commit `8a8c7ed`. Uncommitted.
- **Change.** A monitor's notch, shadow and dock are hidden (HWNDs kept) while all three hold:
  - `SHQueryUserNotificationState` reports BUSY or D3D full screen;
  - the foreground window is foreign, visible and not the desktop;
  - that window covers the monitor's whole rect.
- **When it re-checks.** On catalog generation changes, topology changes, and a 2 s `shortcut:` poll. Microsoft documents that full-screen start and stop send no notification. No new hooks.
- **Verification.** `verify.ps1` PASS: 28 tests plus the release build. Binary `1CC88A122F6A87F1B3890769CFE4786FA72331312B0C124641DCF22B5787D49A`.
- **Clean-code-guard.** Independent non-author reviews:
  - First review on `5BF7C8F2…79B8`: ACCEPT with MIN-1 and MIN-2.
  - Author fixed both.
  - Fresh recheck on `784CD0E22BF902E2E4653B04777AA16EC6896CDCD5614C3304138EFB56D3DD0E`: ACCEPT, no new findings.
- **Open.** Native evidence for real full-screen apps, the multi-monitor foreground split, Presentation Settings, Store apps and the idle cost of the poll. The deferrals of council and accessibility checks are unchanged.

## Sol 6.1 Medium performance judgment of 1e47cc2 (2026-11-10 local)

- **Reviewer.** Independent read-only Sol 6.1 Medium judge, briefed to rank performance and tiny footprint first, at the user's request: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A744ae178-f93d-4e15-bed0-5b05a2a9a815%3Adelegate-task%3Ab0babab3-a2d3-4c1a-b3a2-403feae72e62>.
- **Inputs.** HEAD `1e47cc22801a74bf8a982dd442887c28dd204d0f`. SHA-256: host.rs `D1EF23B1…21AB`, native_window.rs `8EE846F5…3A3D`, topology.rs `D2D49905…F778`. check.ps1 PASS.
- **Verdict: REVISE.**
  - **SOL-PERF-MAJ-1:** the timeout for the fullscreen poll truncates a sub-millisecond remainder to 0, which can busy-wait up to <1 ms per deadline. The author confirmed it. The same truncation exists in `topology_wait` from 8a8c7ed. `CatalogTiming` already rounds up.
  - **SOL-PERF-MAJ-2:** the unconditional 2 s poll adds about 1,800 deadlines per hour without cost evidence. Recommended replacement:
    - one `EVENT_OBJECT_LOCATIONCHANGE` subscription scoped to the foreground PID/TID, out-of-context, skipping ROQA's own process, retargeted on each foreground change;
    - shell-state polling only while the foreground covers its monitor;
    - existing catalog, topology and repair events as fallback.
  - **SOL-PERF-MIN-1:** hidden surfaces still paint and commit.
  - **SOL-PERF-MIN-2:** avoidable String allocations in the fullscreen check.
  - **SOL-COR-MIN-1:** a failed class-name query is not fail-open.
- **Open.** No fixes applied yet. All findings are open. Native idle and trace measurements remain unrun.

## Sol findings confirmed by Opus; fixes submitted (2026-11-10 local)

- **Opus confirmation.** The user asked for the Sol findings to be confirmed with Opus 5.5 High before fixing. A fresh independent read-only Opus 5.5 High verifier on the same HEAD/hashes ran check.ps1 (PASS) and reported:
  - **MAJ-1:** confirmed. `topology_wait` has the same defect. While animating, the zero timeout also drives extra compositor frames.
  - **MAJ-2:** facts confirmed, Major on wake count. Sol's foreground-scoped `EVENT_OBJECT_LOCATIONCHANGE` design was **disputed**, because out-of-context delivery still wakes ROQA for the foreground app's caret, cursor and scroll events. Opus recommended a geometry-first, candidate-gated poll instead.
  - **MIN-1:** confirmed.
  - **MIN-2:** confirmed, a nit.
  - **SOL-COR-MIN-1:** partly confirmed, a nit.
  - Also noted: only one fullscreen monitor is tracked, a documented scope limit.
- **Author fixes (Claude Opus 5.5 High), uncommitted on top of 1e47cc2.**
  - Shared round-up `catalog_timing::milliseconds_until`, used by the catalog, fullscreen and topology waits, with a test.
  - `fullscreen_candidate()` (foreground geometry first) plus `shell_fullscreen()`. The 2 s shell poll is armed only while the foreground covers its monitor, which means 0 fullscreen wakes on an ordinary desktop. In-place resize detection waits for the next catalog generation, 30 s worst case; this is recorded as a `shortcut:`.
  - Hidden surfaces settle at rest and skip paints, and are painted before they are shown again.
  - Allocation-free class-name comparison. The device name is allocated only after the coverage check.
  - A failed class-name query fails open.
- **Verification.** `verify.ps1` PASS: 29 tests plus the release build. Binary `78CFE1E5938D2362B4DF5409270D8854E56DB7038D4C3D5EF782DD32E03E161A`. Manifest `artifacts/m3-fullscreen-2/inputs.sha256`, SHA-256 `66B76864B95F6A86CD2A601807C08AB23AA867E12DF89A7676D69F8589AFC4ED`.
- **Pending.** Fresh independent Sol 6.1 Medium recheck, which also serves as clean-code-guard: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A744ae178-f93d-4e15-bed0-5b05a2a9a815%3Adelegate-task%3Aafbb1cc2-2cf7-4463-8d10-a8b7920c899c>. No native run.

## Sol recheck of the performance fixes: POLISHED (2026-11-10 local)

- **Verdict.** The fresh independent Sol 6.1 Medium recheck returned **POLISHED** (no open blocker/major) on manifest `66B76864…C4ED`.
- **Verification.** All 23 entries verified, check.ps1 PASS (29 tests).
- **Findings.** MAJ-1, MAJ-2, MIN-1, MIN-2 and COR-MIN-1 are all resolved, and there are no new findings. The reviewer agreed with Opus's objection to `EVENT_OBJECT_LOCATIONCHANGE`: `SetWinEventHook` filters only by process or thread, so object filtering happens after delivery. The candidate-gated design is justified within its documented latency trade-off.
- **Clean-code-guard.** The same pass covered it: 0 critical, 0 important, 0 nits.
- **Pre-existing note, not a finding.** `clock::now()` locale-formats strings on every loop wake, including animation wakes.
- **Scope.** This is static review only. CPU/wake counts, allocations, private bytes, GPU/DWM, hidden-surface quiescence, restore presentation and multi-monitor transitions need native measurement.
- **Not done.** No council or milestone acceptance. Uncommitted.

## M3 performance fixes committed (2026-11-10 local)

- **Author.** Claude Opus 5.5 High commit worker, launched by the Chief Thread at the user's request to commit and push. Not a reviewer; this entry approves nothing.
- **Stale-input check.** Manifest `artifacts/m3-fullscreen-2/inputs.sha256` SHA-256 `66B76864B95F6A86CD2A601807C08AB23AA867E12DF89A7676D69F8589AFC4ED` matches; all 23 entries match the working tree (0 mismatches).
- **Verification.** `verify.ps1` PASS right before the commit: checks (29 passed, 0 failed) plus the release build. `roqa.exe` SHA-256 `78CFE1E5938D2362B4DF5409270D8854E56DB7038D4C3D5EF782DD32E03E161A`, identical to the recorded hash.
- **Contents.** The POLISHED candidate, unchanged: `src/catalog_timing.rs`, `src/host.rs`, `src/native_window.rs`, plus this ledger entry. `.gitignore` is the user's own edit and is excluded.
- **Still NOT RUN / deferred, not passed.** Native suites, idle and trace measurement, council review and accessibility.

## M3 clock formatting gated to once per minute: POLISHED candidate (2026-11-10 local)

- **Author.** Claude Opus 5.5 High worker, launched by the Chief Thread at the user's request. It approves nothing. Packet: `docs/development/m3-clock-candidate-1.md`.
- **Change.** Uncommitted, on base `9e449d2`, in `src/host.rs` and `src/native_window.rs`.
  - `refresh_clock` now formats only after a next-minute `Instant` deadline passes. Other wakes skip `GetLocalTime`, the locale formatting and both `String` allocations.
  - `REPOSITION`, now also posted on `WM_TIMECHANGE`, forces a refresh on the next pass.
  - The repaint check now compares the date text as well as the time.
- **Verification.** `verify.ps1` PASS: 29 tests plus the release build. `roqa.exe` `595E7AA2B4CD993135B6EE50D6DFCC346CD59F875E0A1CC3AF1D1D2D3AD85581`.
- **Review.** Independent Sol 6.1 Medium, which also served as clean-code-guard, ran 3 fresh rounds.
  - Rounds 1 and 2 were POLISHED, each with one comment-accuracy Nit (M3C1-SOL-001 and -002). Both were fixed.
  - Round 3 was **POLISHED with 0 findings** on manifest `artifacts/m3-clock-1/round-3/inputs.sha256`, SHA-256 `FDD3B1D07C560F42709C08C62EBEC52AFD8F5FA5F6C9B88B4C04D103F6610D24`.
- **Still NOT RUN / deferred, not passed.** Performance measurement: the saving is static reasoning only. Also native resume, time and locale delivery, council review, native suites and accessibility.

## M3 clock fix committed (2026-11-10 local)

- **Commit worker.** Claude Opus 5.5 worker, launched by the Chief Thread at the user's explicit request ("do all"). It is the commit worker, not a reviewer, and approves nothing.
- **Stale check.** The round-3 manifest hash, every listed `src/` entry and `candidate.diff` matched the working tree before commit.
- **Verification.** `verify.ps1` PASS right before commit: checks (29 tests passed, 0 failed) and the locked release build. `roqa.exe` `595E7AA2B4CD993135B6EE50D6DFCC346CD59F875E0A1CC3AF1D1D2D3AD85581`, identical to the expected hash.
- **Contents.** The POLISHED candidate, unchanged: `src/host.rs`, `src/native_window.rs`, plus the packet and this ledger entry. `.gitignore` is the user's own edit and is excluded.
- **Still NOT RUN / deferred, not passed.** Native suites, idle and trace measurement, council review and accessibility.

## M4 media preparation plan: POLISHED and committed (2026-11-10 local)

- **Author:** a Claude Opus 5.5 High worker thread. Doc-only, base `402941c`.
- **User decisions:**
  - Always animate, including when reduced motion is on; PLAN.md amended. Focus return and accessible actions are still required.
  - Collapsed look A, the static glyph: zero media repaints while collapsed; the existing once-a-minute clock repaint remains; the accepted 150×30 pill is unchanged with no title.
  - The visualizer runs only while visible, expanded and playing.
  - Reviewer plan: one Sol round, then Opus.
- **Reviews:**
  - Sol 6.1 Medium, stale start on `7431797E…`: REVISE, BLK-1 (input changed mid-review), superseded. Link: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Ad8024b42-f1c4-4fc7-9f2b-2000f409518b%3Adelegate-task%3A01e6e9f5-d3c9-4e62-afcd-3e87a8aa357e>.
  - Sol 6.1 Medium on `3BAE462E6D1D28B1D91E82243BACEEF8374352D701EA103400573CC709E28014`: REVISE, with MAJ-1 collapsed clock repaint, MAJ-2 `GetChannelsPeakValues` channel count, MAJ-3 non-Send thumbnail stream handoff, and MIN-1 progress deadline. Link: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Ad8024b42-f1c4-4fc7-9f2b-2000f409518b%3Adelegate-task%3Aafd0e810-e083-43cb-b433-b6f1183ec264>.
  - The author fixed all four.
  - Fresh Opus 5.5 High recheck on `973DDBD23DD9183251153BC413F7A3FC5DCA1739EC4841F3D4E09933DB2D115F`: POLISHED, manifest 19/19 at both its start and end check. Link: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Ad8024b42-f1c4-4fc7-9f2b-2000f409518b%3Adelegate-task%3Ab9dd2da9-263c-4a0b-a704-dfcc897eed38>.
- **Open minor:** M4PREP-OPUS-MIN-1, a progress `m:ss` lag of ≤0.8 s, deferred to slice 4.
- **UNVERIFIED:**
  - unpackaged GSMTC access versus the `globalMediaControl` capability;
  - handler and completion threads;
  - blocking getters and thumbnail reads on a hung player;
  - the `AgileReference` handoff;
  - Core Audio and pool threads;
  - player rate and last-updated fields;
  - abort-on-panic at callbacks;
  - meter cost;
  - all footprint estimates.
- **Open for the user:** collapsed title or wider pill; the endpoint-wide visualizer level; panel size; keyboard route; volume scope; seek UI.
- **Still NOT RUN or deferred, not passed:** council, accessibility, Windows 10, compositor/GPU/DWM/power, and M4 native evidence.
- **Committed by:** the commit worker (a Claude Sonnet 5.5 worker launched by the Chief Thread at the user's explicit request, "do all"), not a reviewer. The stale check passed 19/19 against the final manifest before commit.

## M3 native validation 1: suites PASS, idle CPU inconclusive (2026-11-10 local)

- **Author.** A Claude Opus 5.5 worker, launched by the Chief Thread. Evidence only: it approves nothing. Packet: `docs/development/m3-native-validation-1.md`. Evidence is in `artifacts/m3-native-1/`.
- **Confirmations.** The user confirmed a free desktop twice ("yes the desktop is free", "do all"). Every script ran after a read-only pre-check: foreground, input-idle time, and no ROQA or cargo process.
- **Suites.** All five passed on root `402941c` `595E7AA2B4CD993135B6EE50D6DFCC346CD59F875E0A1CC3AF1D1D2D3AD85581`, each with a graceful `--quit`:
  - `topology-check.ps1`: 25/25 (two monitors, 6/6 surfaces);
  - `smoke.ps1`: 329/329;
  - `picker-smoke.ps1`: 226/226;
  - `catalog-smoke.ps1`: 67/67;
  - `packaged-hung-check.ps1`: 16/16.
- **Baselines.** Built from `git archive` exports with the pinned toolchain:
  - `1e47cc2`: `F00B5824…3B82`;
  - `9e449d2`: `0DB85A44…67EC`;
  - `402941c`: `796DB5C0…1C9C`.

  The `402941c` archive build matches the root build except for 24 bytes of linker timestamp and PDB GUID; the code is identical.
- **Idle observation.** `measure-idle.ps1` ran 3 interleaved repetitions per binary.
  - CPU is **inconclusive**: every run was 0 to 3 ticks of 15.625 ms, at or below the 0.52 ms/s floor of a 30 s sample.
  - Private bytes (about 17 to 21 MiB) and handles (435 or 436) are flat across commits.
  - No performance gain is claimed.
- **NOT RUN.** The `idle-matrix.ps1` helper (GDI/USER at idle, plus context switches as a wake proxy) did not run: its input gate stopped it, because the user was active. Also NOT RUN: the 15-minute trace and fullscreen cases.
- **Deferred.** DWM, GPU, power, council review, accessibility and Windows 10.

## M3 surface roles split (M3T1-CCG-MIN-3): ACCEPT, committed (2026-11-10 local)

- **Author:** an Opus 5.5 High worker; packet `docs/development/m3-surface-roles-candidate-1.md`.
- **Change:** `Surface` notch-only and dock-only fields moved into `enum Role { Notch, Dock }`, with `kind` derived from the role.
- **Footprint:**
  - `size_of::<Surface>()` went from 320 to 256 bytes, measured;
  - notches no longer allocate unused slot tweens or a UIA Arc;
  - no per-wake work added.
- **Review:**
  - fresh independent Opus 5.5 High clean-code-guard, round 1: ACCEPT on `13C60C00748CEEE65286D6ABE77C4404B20890B72281951D3272A636BF2D237F` (`artifacts/m3-surface-roles-1/round-1/inputs.sha256`; its `src/host.rs` entry `67879567...D1C1` is the committed file);
  - M3T1-CCG-MIN-3 is resolved;
  - comment-only nits M3SR-OPUS-NIT-1 and -2 are deferred to the next `host.rs` candidate;
  - review link: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A09bc86ad-2306-4226-8c8a-23010320204f%3Adelegate-task%3Af0f0c050-892c-4103-8f5e-11b60b729d9c>.
- **Verify:** `scripts/verify.ps1` run in root before commit: checks PASS (29 tests, 0 failed) and release build PASS. Release `roqa.exe` SHA-256 `1F61CABB399DECFC263B34431D88C1FB02933A6EE950B74541F1571316F89227`. This differs from the reviewed build `C757803F...36FA`; the base changed (docs only) and the build timestamp differs, so the binary was not byte-compared with the reviewed one.
- **Still NOT RUN on this binary:** native suites; also council, accessibility and idle trace.
- **Committed by:** the commit worker, not a reviewer.

## M3 measurement 2: finer idle counters in `measure-idle.ps1`, ACCEPT; 3×3 run 1 done (2026-11-10 local)

- **Author.** A Claude Opus 5.5 worker launched by the Chief Thread. Packet: `docs/development/m3-measure-2.md`. Evidence: `artifacts/m3-measure-2/`.
- **Change.** `scripts/measure-idle.ps1` only, with no production change. All existing output fields are kept. New:
  - `QueryProcessCycleTime` cycles/s (not tick-quantized), with an **approximate** ms/s figure at WMI `MaxClockSpeed`;
  - GDI and USER objects at start and end;
  - a per-thread WMI context-switch **wake proxy**, read outside the CPU window;
  - `-SampleSeconds` (default 30);
  - a STOP on any user input;
  - a guard that refuses to run into the display-off or screen-saver timeout. It only reads settings.
- **Not strictly M1-comparable.** The settle is now about 10 s (`SettleSeconds`).
- **Self-check.** `artifacts/m3-measure-2/selfcheck.ps1` runs the script's own statements and helpers without launching ROQA, and passes:
  - 0 parse errors;
  - the C# compiles;
  - the guard refuses 86400 s;
  - the cycle, GUI, power and WMI helpers work;
  - the per-thread delta cases pass.
- **Review.** Each round was a fresh, independent, read-only Opus 5.5 High clean-code-guard reviewer. No Sol was used.
  - **Round 1: REVISE.**
    - 2 MAJOR: the wake proxy was biased low when threads exit; the 900 s display timeout falls inside a 15-minute run.
    - 5 MINOR and 4 NIT.
    - Manifest `BB67F5BA…D659DB`.
    - Link: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Ab6103d3e-035d-476c-ad32-4f2bda502186%3Adelegate-task%3A5edd64a6-68b8-4c6c-beca-387828f3c986>.
  - **Round 2: ACCEPT WITH MINORS.**
    - New: 2 MINOR (0 GDI threw; an empty WMI read passed as zero wakes) and 3 NIT.
    - Manifest `11FC563C…5884D7`.
    - Link: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Ab6103d3e-035d-476c-ad32-4f2bda502186%3Adelegate-task%3Aeb8e471a-bad4-4eeb-85ea-46ddafe6189a>.
  - **Round 3: ACCEPT.** All R2 findings were resolved.
    - Manifest `artifacts/m3-measure-2/round-3/inputs.sha256` = `E06990606CEDB4E793BCF85CABA1A016ECA43C2F99E2FA99495A9B58B154AC8B`; 6/6 at both start and end.
    - Its `scripts/measure-idle.ps1` entry is the current file.
    - Link: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Ab6103d3e-035d-476c-ad32-4f2bda502186%3Adelegate-task%3Ab41c4790-7c83-4897-878b-6dc154830643>.
- **Deferred NITs (Limits wording only).**
  - M3M2-R3-NIT-1: the group-policy display-off timeout is not listed as unread.
  - M3M2-R3-NIT-2: the power source is read once, before launch.
  - M3M2-R3-NIT-3: `ScreenSaveActive` is ignored. The guard errs toward refusing.
  - Also open and documented: R1 NIT-4, an old `idle.json` survives a STOP. This predates the change.
- **This machine.** The AC display-off timeout is 900 s, so the guard refuses a 900 s sample. The longest accepted sample is about 860 s with fresh input. A full 15-minute run needs the user to decide on their display timeout; nothing was changed.
- **Binaries frozen for the next run:**
  - `1e47cc2` `F00B5824…3B82`;
  - `9e449d2` `0DB85A44…67EC`;
  - `26cbb0a` `1F61CABB…9227`, copied to `artifacts/m3-measure-2/bin/26cbb0a/`.
- **Run 1 (3×3, 30 s, 2026-11-10 about 15:42 to 15:49 UTC):** 9/9 complete, with no STOP and no input. Builds are **not distinguishable**: cycles/s medians are `1e47cc2` 1.58M, `9e449d2` 1.45M and `26cbb0a` 0.77M, but every build's range spans about 0.77M to 1.69M. The split into two clusters matched a wall-clock minute boundary inside the sample in 9/9 runs; that it is the clock repaint is a hypothesis. Approx. 0.20 to 0.43 ms/s, unverified clock basis. Wake proxy 1.6 to 3.8 switches/s. GDI 42, USER 21 and handles about 440→442 in every run. Private bytes about 20 MB. Not an M5 gate verdict. Details: `docs/development/m3-measure-2.md`, "Run 1 results"; evidence `artifacts/m3-measure-2/run-1/`.
- **NOT RUN.** The long (up to about 860 s) run needs a fresh desktop-free confirmation from the user. No reviewer has checked run 1.
- **Deferred.** Council review, accessibility, Windows 10, and DWM/GPU/power.

## M3 resilience candidate 1 (sleep/resume, lock/unlock, Explorer restart): ACCEPT (2026-11-10 local)
- **Author:** Claude Opus 5.5 High worker; base `26cbb0a`, branch `feat/m3-resilience`; packet `docs/development/m3-resilience-candidate-1.md`.
- **Change:** the resume broadcast (`WM_POWERBROADCAST`/`PBT_APMRESUMEAUTOMATIC`) and the registered `TaskbarCreated` message now post the existing coalesced `REPOSITION`, through a pure, tested `repositions` predicate. The unused `dpi` value is removed. No hooks, threads, timers, session registration or crate features; zero added idle wakes. Lock/unlock: no documented failure, so no handler; WTS registration is the follow-up if native evidence fails.
- **Verify:** PASS, 30 tests; `roqa.exe` `84641A1917AEDBB0EE6BB22C9AE7D832D4389513898A2D2707174EFBF302C4F0` (worktree build).
- **Review:** fresh Opus 5.5 High clean-code-guard, 3 rounds. Final ACCEPT with 0 findings on `8D25DB6E22BBB2B646EC9998EB2C20AFF415042A57B0A22EB95E080320016612` (`artifacts/m3-resilience-1/round-3/`). Nits M3R1-OPUS-NIT-1 and M3R2-OPUS-NIT-1 fixed; M3R2-OPUS-NIT-2 declined.
- **Landed on main:** a separate commit worker re-checked the worktree files 9/9 against the round-3 manifest, and the live diff equals `candidate.diff`. The worktree commit `96a580d` was cherry-picked onto `8f5410a`. The Rust blobs are stored LF; `host.rs` is CRLF on disk and git normalizes it. Root `scripts/verify.ps1` was run just before the commit: checks PASS (30 passed, 0 failed) and build PASS. Root release `roqa.exe` SHA-256 is `1A613E813C53567C89EB99DC23277AE7EDDD135EE5CABA1C56E4733598D152B02`. This differs from the worktree build (different path and base); the two were not byte-compared.
- **NOT RUN / deferred:** all native resilience cases (sleep/resume, lock/unlock, and Explorer restart, which needs separate shell-affecting authorization), suite regression, council, accessibility, Windows 10, Modern Standby/hibernate, device loss, DWM/GPU/power.

## M3 device-loss candidate 1 (graphics device-loss recovery): ACCEPT WITH NITS (2026-11-10 local)
- **Author:** Claude Opus 5.5 High worker; base `6f665cf`, branch `feat/m3-device-loss`; packet `docs/development/m3-device-loss-candidate-1.md`.
- **Before:** any D2D/DComp paint failure propagated to `main` and exited 1; no recovery, and idle loss went unnoticed until the next paint.
- **Change:** loss is detected from paint/Show results (`Graphics::lost`: DXGI REMOVED/RESET/HUNG/DRIVER_INTERNAL_ERROR, D2DERR_RECREATE_TARGET, or `GetDeviceRemovedReason`/`CheckDeviceState`) and from the WM_PAINT that DComp sends on loss. Recovery drops all surfaces, creates new D3D/D2D/DComp devices, re-shows the shown kinds once, then re-reads topology and the clock. The `Rebuild` schedule backs off 250 ms→8 s; 8 failed attempts without a settled minute exit visibly. No thread, timer, poll or crate feature; zero added idle wakes. Debug-only `--probe-device-loss`.
- **Verify:** PASS, 33 tests; release `roqa.exe` `2D28A19A2B6D25004EBBCD169E212CBC86C2E139271ED70A831C14CBD04C691C` (worktree build).
- **Review:** fresh Opus 5.5 High clean-code-guard, 2 rounds. R1 CHANGES REQUIRED (2 MAJOR, 2 MINOR, 2 NIT; all fixed or documented). R2 ACCEPT WITH NITS on `DC339882F7A8063094A00125A22BE3B7717223F38CA5435C8EE1460BE4BB2DCF`; 3 nits deferred (rebuild logging, `finish(bool)`, a pending-rebuild topology edge).
- **NOT RUN / deferred:** all native cases (debug probe; real TDR and remote-session switch need separate system-affecting authorization), suite regression, idle sample, council, accessibility, Windows 10, DWM/GPU/power.
- **Landed:** on main via cherry-pick; root verify PASS, 33 tests; root release roqa.exe FCD1646C3B172F0421901DF883581427403828E4868128CD84F75C6C7C2B3E0F (differs from worktree build; not byte-compared).

## M4 media state candidate 1 (slice 1: GSMTC state, no UI): ACCEPT (2026-11-10 local)
- **Author:** Claude Opus 5.5 High worker; base `0b53246`, branch `feat/m4-media-state`; packet `docs/development/m4-media-state-candidate-1.md`.
- **Change:** new `src/media.rs` (pure `View` reducer + host-thread GSMTC owner: `RequestAsync` via `when`, manager/session event tokens, session generation, `WM_APP+50` wake, ordered teardown, length-only diagnostics). `Desktop.media` declared before `text`; started after first paints. Feature `Media_Control`; `windows-future =0.3.2` direct dep (already locked). No UI change, no timer/poll/deadline/thread; zero added idle wakes by construction.
- **Verify:** PASS, 42 tests; release `roqa.exe` 656,384 bytes `7520DE68B7622E283599B7B1009466056DF46BDD78F54AB2FB2D7CD2200EA445` (worktree build); +39,424 bytes (+6.39%) over base 616,960.
- **Review:** fresh Opus 5.5 High clean-code-guard, 2 rounds, no Sol. R1 ACCEPT WITH NITS (3 MINOR, 6 NIT; 8 fixed/documented, NIT-5 declined). R2 ACCEPT, 0 findings, on `5F92048240958E927FBBB8A4997AA5FD9279652CAD06E0C64A1ACD02B217F7BE` (`artifacts/m4-media-state-1/round-2/`).
- **User decisions (2026-11-10):** collapsed pill shows the title (wider pill, static end-ellipsis); play/pause/prev/next controls; settable volume; taller expanded panel (~360x120); keyboard via Tab/arrows + UIA; seek by bar click; endpoint-wide visualizer. Recorded in `docs/development/m4-media-preparation.md`. Design mockups skipped by user choice ("start blind"); design to be improved later.
- **UNVERIFIED:** unpackaged GSMTC access; callback threads; blocking getters/hung read; session identity stability; rate/last-updated population; event rate; pool threads; startup and private bytes.
- **NOT RUN / deferred:** all native evidence (packet plan), suite regression, idle sample, council, accessibility, Windows 10, DWM/GPU/power.
- **Landed:** on main via cherry-pick; root verify PASS, 42 tests; root release roqa.exe `0187069060DA66E08900EFC9F3F2CEC59C75AC9DF0879FB8B923E124F632F4F8`, 656,384 bytes (differs from worktree build; not byte-compared).

## M4 media paint candidate 1 (slice 2: static media paint, direction B): REVIEWED, NATIVE EVIDENCE PENDING (2026-11-10 local)
- **Author:** Claude Opus 5.5 High worker, continuing an unfinished earlier worker's WIP (thread `mcp:launch:0b29a3e5…`); base `478d9e0`, branch `feat/m4-media-paint`; packet `docs/development/m4-media-paint-candidate-1.md`.
- **Change:** direction B "Cover accent" (user choice 2026-11-10, PROVISIONAL addendum in `docs/design/roqa-visual-spec.md`): 262x30 media pill (cover slot + play-state glyph, end-ellipsis title, measured clock), 360x120 panel (72 px cover, title/artist/clock; progress/controls/volume rows reserved empty); no art (slice 5); repaint only on drawn-view change (`media::redraw`), no timers/deadlines/threads; notch HWND 400x124 -> 440x164 DIP (surface cost estimated, not measured); smoke/topology updated to 440x164, smoke no-media INTERFERENCE guard.
- **Verify:** PASS, 55 tests; release `roqa.exe` 664,576 bytes `25193D312AD5DF23435E23EDB1063A67B1DCB88D46D1CD89D68DFA90407EE3DD` (worktree build); +8,192 bytes (+1.25%) over base 656,384.
- **Review:** fresh Opus 5.5 High clean-code-guard, 1 round, no Sol. APPROVE WITH NITS (1 MINOR deferred to native run, 4 NIT accepted) on `004777564cdc29985e7694205c0dcfefc8758384f28d7baaf674a6bf8a759b3f` (`artifacts/m4-media-paint-1/b-round-1/`). Addendum reviewed with the paint, not separately (Chief decision, user chose "start blind").
- **Native evidence already collected (slice 1, by the user, 2026-11-10 on `478d9e0`):** `Media: RequestAsync succeeded`; real `playing session=1` transitions; `callback_on_host=false`. Unpackaged GSMTC access works; callbacks arrive off the host thread.
- **UNVERIFIED:** all on-screen appearance and animation, RTL ellipsis side, media-shape input region, static paint counts, surface memory/CPU, base size.
- **NOT RUN / deferred:** native evidence (packet plan), 5-suite regression, council (Sol 6.1 medium + Opus 5.5 medium requested by the user, next), accessibility, Windows 10, DWM/GPU/power, 15-minute trace.
- **Landed:** on main via cherry-pick; root verify PASS, 55 tests; root release roqa.exe 3F385C7D9B60A5CB208D3A96EE426AE68C1E1A77EE98E39D98877A8D3CC6B911, 664,576 bytes (differs from worktree build; not byte-compared).

## src architecture review 1: native assessment frozen; independent assessment pending (2026-11-10 UTC)

- **Author/reviewer:** Sol 6.1 Medium architecture worker; source author is not this reviewer. Scope: all 20 src Rust files at 8eb909b, existing docs (user B). No production edit.
- **Skill:** gstack plan-eng-review, upgraded install 1.97.0.0 to 1.99.0.0 (f0cd9f8c); root/project .gitignore untouched.
- **Report:** docs/development/reviews/src-architecture-review-1.md. **Exact raw-byte manifest:** docs/development/reviews/src-architecture-review-1.inputs.sha256, 37 entries, SHA-256 CB5467952366F4BD5E616832DE6D1874BEA1C9ADF1B18A5414FED0C2673A06EC. Ledger excluded from its own inputs.
- **Recommendation:** preserve the current event-driven stack and ownership; assess bounded visible-icon demand/cache convergence first (AR1-A1/T1). Keep known synchronous platform-read uncertainty explicit (AR1-A2); carry drawn-view contract caution into later M4 work (AR1-Q1). No rewrite or speedup claim.
- **Next:** fresh independent Opus 5.5 High assessment of report and identical inputs, then only justified bounded fixes, non-author clean-code-guard/checks, authorized local commits and single-writer integration. Start the next bounded M4 slice after that gate; no push. User's latest direction supersedes earlier no-commit instruction.
- **Evidence:** static source/docs/history only; cargo checks NOT RUN by this reviewer. Native GUI requires fresh desktop-free confirmation for every run; keyboard/accessibility explicitly DEFERRED/NOT RUN. All other existing runtime/council limitations remain open; this is not milestone acceptance.

## src architecture review 1: bounded cache patch independently approved (2026-11-11 Africa/Cairo)

- **Author:** Sol 6.1 Medium; base 8eb909b7b5825fcba2d4f8ff8c81e5536852ffa0. Skill update/native freeze remain recorded above. Successor closure: docs/development/reviews/src-architecture-disposition-1.md; original report/manifests remain immutable historical evidence.
- **Independent architecture assessment:** fresh Claude Opus 5.5 High, docs/development/reviews/src-architecture-opus-1.md, SHA-256 07AD1E6FBDF4E15B6C7F90E4F8BFA8290A79FEDC061E7D50903C6D39AD7EE63D. All 37 original entries verified at start/end. Task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Ad7b27b05-abe9-4d27-97e4-0c6708ac6651%3Adelegate-task%3A36aa3db2-274e-4c25-a485-ab73ea5bf296>.
- **Dispositions:** AR1-A1 severity/remedy revised; visible-prefix optimization deferred. Accepted AR1-OPUS-1/2: cap cached+pending at 256, evict closed apps on changed dock list, validate completion size before retiring pending work, require pending membership. No new dependency/thread/timer/wake or polling. AR1-A2/OPUS-3 platform latency/shutdown remains unknown; AR1-Q1 carries into M4 controls/progress. AR1-T1 covered by four channel-only regression tests.
- **Candidate:** docs/development/icon-cache-candidate-1.md; exact 39-input manifest docs/development/reviews/icon-cache-candidate-1.inputs.sha256 SHA-256 1B4FADE410BE3B8261E6DAB219525F2AC01788F23AE21AB4BC8C9EA255FF207D.
- **Fresh independent clean-code-guard:** Opus 5.5 High, docs/development/reviews/icon-cache-guard-1.md SHA-256 DFA75A9921E83103C5023BED5648130C222A390FFF2EE633787A60FF5D9658A2. All 39 entries verified start/end, HEAD unchanged. APPROVE WITH NITS; AR1-OPUS-1/2 and T1 resolved on exact inputs. Task: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Ad7b27b05-abe9-4d27-97e4-0c6708ac6651%3Adelegate-task%3A6a20210d-8a80-4b29-a84c-8a0a4ea40936>.
- **Deferred guard findings:** ICG1-1 P3, after every queued app closes a new app can keep its placeholder until another dock event; no leak/extra wakes, nonblocking. ICG1-2 shortcut-cap comment and ICG1-3 test style nits deferred; no reviewed-source churn. clean-code-guard: 1 minor, 2 nits, no blocker.
- **Verification:** author frozen scripts/verify.ps1 PASS checks + release, 59 tests; reviewer independently reran scripts/check.ps1 PASS fmt/clippy/59 tests. Earlier verification concurrent with author edits is superseded. Final verify immediately before code commit and integration verification are required and recorded in the next ledger entries. Generated logs/binaries/caches excluded from commit.
- **Authorization/limits:** user now authorizes local commit and single-writer root integration, preserving/excluding user's root .gitignore; no push. After integration delegate next bounded M4 slice 3 controls to Opus 5.5 High, separate candidate/review/check gate. This is not milestone/council acceptance. Native GUI requires fresh desktop-free confirmation EACH run and foreground inspection; keyboard/accessibility explicitly DEFERRED/NOT RUN. Native performance, real shell/player and all other deferred evidence stay unpassed.

- **Final pre-commit verification (2026-11-11 Africa/Cairo):** scripts/verify.ps1 -EvidenceDirectory artifacts/verification/icon-cache-1-precommit completed PASS (checks/release exit 0; 59 tests). Candidate source unchanged since independent guard; all 39 manifest entries revalidated before this run. Local source commit follows this gate; no native/GUI test or acceptance claim.

## Icon cache candidate 1: integrated on root main (2026-11-11 Africa/Cairo)

- **Integrator:** Claude Opus 5.5 High, delegated sole root writer; integration only, not review or acceptance. Record: `docs/development/icon-cache-integration-1.md`.
- **Source:** `3d6ffc66b5a0bd52aa11ae8cd54efd46a74dc5a0` (Sol 6.1 Medium; guard APPROVE WITH NITS). Root `main` was `8eb909b`, not advanced; clean cherry-pick `--no-commit`, no conflict. Integrated commit `0e1105c8741576f34a46904ef9e6afe9ce1a8337`; tree identical to `3d6ffc66`.
- **Encoding-only restoration:** 27 files (autocrlf CRLF checkout) restored byte-for-byte from the parent working tree after confirming each normalized blob equals the root index and `3d6ffc66` blob; no content change.
- **Inputs:** manifest `icon-cache-candidate-1.inputs.sha256` (`1B4FADE4…55FF207D`) 39/39 OK on root before verification and before commit.
- **Verify:** root `scripts/verify.ps1 -EvidenceDirectory artifacts/verification/icon-cache-1-integrated` PASS: checks (fmt, clippy, 59 passed, 0 failed) and release build. Root release `roqa.exe` `F55FAFCD006135E072E0EDA0BD39F95177ACE83027B8190AB58F327B0D3E70AD`, 666,112 bytes (differs by path/base from worktree build; not byte-compared).
- **Preserved:** user root `.gitignore` modification untouched, unstaged, SHA-256 `48AAA74C…885313DAC` before and after.
- **Deferred:** ICG1-1/2/3 unfixed by direction. Native GUI, keyboard/accessibility, runtime performance, council: DEFERRED/NOT RUN. No push. Next: parent delegates M4 slice 3 controls to Opus 5.5 High under its own gate.
