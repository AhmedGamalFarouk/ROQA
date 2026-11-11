# Release readiness: decision and execution checklist

Status: engineering candidate only; no milestone or release acceptance. Owner: Chief. Updated 2026-11-11.
This plan does not authorize buying certificates, selecting a license, publishing, installing updates,
registering startup, replacing the taskbar, or changing the system.

| Gate | Concrete next action / evidence | State |
|---|---|---|
| Repository license | User selects proprietary, permissive, copyleft, or another reviewed license; record copyright owner and contributor terms; add the actual license only after that decision. | OPEN: user decision |
| Dependency rights | Export pinned Cargo.lock dependency/license inventory; inspect transitive licenses, Windows SDK redistribution and any bundled assets; produce third-party notices for the selected distribution. | OPEN |
| Supported platforms | Validate Windows 11 x64 against the reference hardware/build matrix; decide whether to support Windows 10 or ARM64 and test each supported target. | OPEN |
| Independent code gate | Chief requests fresh independent Opus 5.5 High review of the frozen implementation manifest, fixes material findings, and freezes/rechecks changed inputs. Complete required M4/M5 council/CCG gates on identical manifests; static acceptance cannot replace runtime gates. | OPEN |
| Native correctness | Fresh desktop-free confirmation and foreground check via Chief; rerun smoke using the release host and freshly built debug fault probes. Require zero UIA E_INVALIDARG and bounded handle/USER/GDI churn; retain all failures. | NOT RUN on this candidate |
| Feature/accessibility | Verify shelf add/open/reveal/delete and all 12 items via More; Tab/arrows/Escape, native pin reorder buttons, Narrator names/focus/stale pages, RTL/IME, 100-225% text scale and mixed DPI. | NOT RUN |
| Event/lifecycle | Exercise file picker with timer completion/store flush and quit while open; repeated/hung network clicks; topology change while settings is open; high-contrast/system-color change and restore. Settings/display changes need separate user authorization. | NOT RUN |
| Recovery matrix | Picker suite, catalog, packaged/hung sources, resume/sleep, Explorer restart and device loss; obtain each separately required authorization. No system mutation solely to finish a checkbox. | OPEN |
| Resource/performance | Build before measurement; record source/binary hashes, OS/GPU/display/settings; run 15-minute CPU/private-byte, input latency, multi-monitor/churn and DWM/GPU/power gates in PLAN. Matched pinned Nectar/Nusa repetitions with descendants accounted; report below-floor values as inconclusive. | NOT RUN; no performance claim |
| Signing | User chooses unsigned development delivery, individual/organization certificate or managed signing service; decide publisher identity, certificate custody, timestamping and budget. Test signature verification and SmartScreen behavior; never promise reputation. | OPEN: user decision |
| Distribution | User chooses portable archive, MSIX or a reviewed installer; document install location, per-user/admin scope, prerequisites, file ownership and rollback. Do not publish a package before that decision. | OPEN: user decision |
| Uninstall/data | Prove graceful shutdown and removal of installed files; decide whether uninstall offers to retain/delete ROQA store/error logs. Preserve user files and native taskbar; document manual portable cleanup if selected. | OPEN |
| Updates | User chooses manual release downloads or a separately reviewed updater; if automated, specify signature verification, trusted metadata, downgrade/rollback policy and failure handling before implementation. No updater or startup registration now. | OPEN: user decision |
| Reproducible candidate | Fresh verify.ps1 immediately before commit/landing, exact changed-file manifest + packet, binary size/SHA and commands/results; exclude user .gitignore, binaries/caches/artifacts. scripts/land.ps1 -DryRun then reviewed actual land via mechanical worker; no force-push or hook bypass. | OPEN: Chief integration |
| Release approval | Resolve user choices, dependency notices and required independent/native/performance gates; user authorizes actual publication with destination and version. A source push is not a release acceptance. | OPEN |

## Product backlog (separate from this readiness candidate)

Live DWM previews, jump lists/context menus, badges, audio device switching, timer IPC channel,
timer cancel/pause, notification packaging, lyrics/calendar/control-center integrations, Follow Windows,
per-monitor themes and real glass backdrop blur remain backlog. Global hooks/hotkeys, taskbar replacement,
startup registration and shell mutation require separately reviewed scope. The accepted theme direction
and always-animate choice remain in force.

Per-canvas text-layout caching remains deferred pending a trace: animation changes font size/bounds,
and a cache of those transient layouts adds retention/eviction work. Revision-based UIA label reuse is
implemented without retaining track strings or DirectWrite objects per canvas. No speedup is claimed.

The previously refused deletion of roqa-m2b2/target remains pending. It was not retried; its parent has a
.tools junction to the live toolchain and must never be recursively removed as a cleanup shortcut.
