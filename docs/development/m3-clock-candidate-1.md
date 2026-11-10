# M3 clock candidate 1: format the clock once per minute

- **Author.** Claude Opus 5.5 High worker, launched by the ROQA Chief Thread at the user's request. The author approves nothing.
- **Base.** `9e449d2` (main). Uncommitted. ` M .gitignore` is the user's own edit and is excluded.
- **Changed files.** `src/host.rs` and `src/native_window.rs`.
- **Final manifest.** `artifacts/m3-clock-1/round-3/inputs.sha256`, SHA-256 `FDD3B1D07C560F42709C08C62EBEC52AFD8F5FA5F6C9B88B4C04D103F6610D24`. It covers the 4 source files plus the frozen diff.

## Change

- **Deadline gate.** `Desktop.clock_due: Instant` gates `clock::now()`.
  - Until the deadline, a loop wake makes one `Instant::now()` call and a comparison. It skips `GetLocalTime`, both locale-format calls and both `String` allocations.
  - Once the deadline passes, the clock is formatted once and the deadline is re-armed to `now + next_minute_ms`, which keeps the existing +20 ms margin.
  - The wait is returned through the shared round-up `milliseconds_until`.
- **Repaint rule.** Visible notches repaint when the time or date text changed. Before, only time changes triggered a repaint, so a date-only format change now repaints too.
- **Forced refresh.** `REPOSITION` resets the deadline.
  - It is posted on `WM_SETTINGCHANGE`, `WM_DISPLAYCHANGE`, `WM_DPICHANGED`, and now also on `WM_TIMECHANGE`.
  - So a clock, time-zone or locale change still refreshes on the next pass. Before, any wake did this incidentally.
  - Cost: one coalesced topology read on these rare events, which is a no-op diff when the monitors are unchanged. There is no new hook or timer, and no idle cost.
- **Contracts kept.**
  - All notches show the same minute.
  - `place()` still refreshes before it paints a new notch.
  - Hidden notches skip paints.
  - The +20 ms post-boundary wake is unchanged.
  - Waits never truncate below 1 ms.

## Verification (author, pinned `scripts/verify.ps1`)

| Round | Result | Release `roqa.exe` SHA-256 |
| --- | --- | --- |
| 1 | PASS, 29 tests | `EF366841BF0976B914DA4F6F3D22CAAE1F9173EE0E77C733874E2AD4C1BE419C` |
| 2 | PASS, 29 tests | `780D4B86F460816AA6436AE0654FF113DAF236D6F85CA175BA917D7157CCBBB7` |
| 3 (final) | PASS, 29 tests | `595E7AA2B4CD993135B6EE50D6DFCC346CD59F875E0A1CC3AF1D1D2D3AD85581` |

Logs are in `artifacts/m3-clock-1/verification*/`. No new test was added. The deadline branch is direct, and `Desktop` owns native resources. The round-1 reviewer agreed that a test seam would cost more footprint than it is worth.

## Independent review (Sol 6.1 Medium, also clean-code-guard)

| Round | Manifest SHA-256 | Verdict | Task |
| --- | --- | --- | --- |
| 1 | `7B5612B9…E28BA` | POLISHED, 1 Nit | <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A977f7c07-8bb9-45e1-bbcd-19d7b964a733%3Adelegate-task%3A5dcc7435-07b5-41e7-884a-bb03a0332c94> |
| 2 | `349025F6…57E6` | POLISHED, 1 Nit | <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A977f7c07-8bb9-45e1-bbcd-19d7b964a733%3Adelegate-task%3A2d23b5e1-f6ed-415e-bb2b-f54427237759> |
| 3 | `FDD3B1D0…0D24` | **POLISHED, 0 findings** | <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A977f7c07-8bb9-45e1-bbcd-19d7b964a733%3Adelegate-task%3A8e9f75e1-e42d-4720-8600-b8ff3f5ef753> |

Each round checked the manifest (PASS) and ran check.ps1 (PASS, 29 tests).

- **M3C1-SOL-001 (Nit), fixed.** A `shortcut:` comment claimed that `Instant` might pause during suspend. Pinned Rust uses QPC, which counts suspended time.
- **M3C1-SOL-002 (Nit), fixed.** The replacement comment overclaimed a guaranteed wake after resume. It now uses the reviewer's wording.

## Known limits

- **Not measured.** The saving is static reasoning only. CPU, wakes, allocations, syscalls and binary size were NOT RUN and are not measured.
- **Delivery is unverified natively.** Resume, time, time-zone and locale-change delivery, and multi-monitor pixels, were NOT RUN.
- **Repaint timing.** A repaint can still lag the minute by scheduling delay. There is no hard real-time guarantee, the same as before.
- **Gates still open.** The reviewer ran Sol at Medium, as briefed. The Sol High and Opus High council, native suites, idle/trace measurement and accessibility are deferred and not passed.
