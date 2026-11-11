# M5 native batch Sol 2 — stopped on interference

Base `238c2c853ee2ce4f13acfe6dbac106601504698f`; 2026-11-11. User batch desktop-free authorization supplied in task; no repeat prompt. No builds, production/harness implementation edits, commits, pushes or system changes.

Binary: `target/release/roqa.exe`, 999424 B, SHA-256 `A329BF01972840FC1EBAE04792E00B0E6CEFF1F0093991BCC22831433D70622C`.

- Picker **INCONCLUSIVE**, 2 checks completed; `artifacts/m5-picker-sol-2/picker-smoke.json`. Windows denied initial fixture foreground before ROQA launch. No product verdict.
- Preflight `artifacts/m5-picker-sol-2/preflight.json`: current/input desktop Default/Default; foreground 263998 stable over 600 ms; no ROQA/other suite/measurement; exact base/size/SHA match. Initial preflight self-matched its command-shell script text (no suite launched), retained as `preflight-self-match.json`; artifact-only runner corrected to exclude command wrappers and rechecked.
- Command: `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File scripts/picker-smoke.ps1 -EvidenceDirectory artifacts/m5-picker-sol-2 -SkipBuild`. Suite threw `INTERFERENCE: Windows denied initial fixture foreground.`; wrapper exited 1. Native stderr became a terminating PowerShell NativeCommandError before wrapper exit.txt; suite JSON is complete, suite.log empty. Error retained here, not treated as success.
- Catalog and packaged/hung **NOT RUN**: explicit stop-on-interference instruction. No desktop switch, assertion bypass or retry. Their reserved evidence directories were not created.
- Cleanup `artifacts/m5-picker-sol-2/cleanup.json`: suite PowerShell exited, 0 ROQA processes, fixture HWND 0, picker HWND 0. No process was killed by this worker; no new runtime/UIA cleanup output because ROQA never launched.
- Prior topology **PASS 25 checks**, `artifacts/m5-topology-sol-1/topology.json` (`Result=PASS`, graceful client/host exit 0); prior smoke **PASS 331 checks**, `artifacts/m5-smoke-sol-2/smoke.json`. Both carry the binary SHA above; neither repeated.
- Prior unsuppressed runtime errors: topology 2 and smoke 222 occurrences of `Accessibility cleanup: The parameter is incorrect. (0x80070057)` in their runtime.log files. **UNRESOLVED**, passing assertions do not clear cleanup errors.
- Remaining: picker/catalog/packaged-hung batch, Narrator speech, actual IME composition, OS text scale, broader hardware/lifecycle/manual matrix, native performance and long measurements. No milestone acceptance.

Artifact runner: `artifacts/m5-native-sol-2/run-suite.ps1`. Existing smoke-envelope changes and user's `.gitignore` preserved; no staging. Packet SHA recorded in `m5-native-sol-2.sha256` (packet only).
