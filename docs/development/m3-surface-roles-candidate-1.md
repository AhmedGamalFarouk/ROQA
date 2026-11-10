# M3 surface roles candidate 1: split notch-only and dock-only surface state

- **Author.** Claude Opus 5.5 High worker, launched by the ROQA Chief Thread at the user's request. The author approves nothing.
- **Base.** `402941c` (origin/main), branch `refactor/surface-roles`. Uncommitted.
- **Resolves.** Deferred minor M3T1-CCG-MIN-3 (`docs/development/m3-topology-candidate-1.md`): `Surface` mixed notch-only and dock-only fields.
- **Changed files.** `src/host.rs` only (+140/-89).
- **Manifest.** `artifacts/m3-surface-roles-1/round-1/inputs.sha256`, 10 entries, SHA-256 `13C60C00748CEEE65286D6ABE77C4404B20890B72281951D3272A636BF2D237F`. It covers `src/host.rs`, the modules it reads (`dock_uia.rs`, `graphics.rs`, `native_window.rs`, `layout.rs`, `motion.rs`), `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` and the frozen `candidate.diff`.

## Change

- **Role split.** `Surface` keeps the shared state (canvas, window, monitor, DPI, bounds, hovered/visible/live, motion). Kind-specific state moved into `enum Role`:
  - `Notch { shadow, engaged, intent, area }`
  - `Dock { _accessible, hover, slots, uia }`
- **No `kind` field.** `Surface::kind()` derives it from the role; existing kind checks call it.
- **Small deletions.** `fit_notch_region` was inlined into its only caller, `paint_notch`. `open_notch` became a free function over `(intent, motion)` so it can run while the role is borrowed. The `Option` wrappers around the shadow and UIA root are gone.
- **Kept.** Creation order (shadow, window, UIA or region, canvas). `role` is the first field, so UIA and the shadow are still released before the owning HWND. Hover, intent, animation, UIA publishing, paint order, fullscreen hide/show and hidden-surface skipping, `clock_due` and topology reconciliation are unchanged in logic.
- **Footprint.** `size_of::<Surface>()` went from 320 to 256 bytes, measured with a temporary test on both versions and then removed. Each notch no longer allocates an unused slot-tween `Vec` or UIA `Arc<Mutex<State>>`. Nothing is added per wake: the old field reads become enum-tag matches, with no allocation.

## Verification (author, pinned `scripts/verify.ps1`)

- PASS: fmt, clippy `-D warnings`, **29 tests passed, 0 failed**, locked release build. Logs in `artifacts/m3-surface-roles-1/verification/`.
- Release `roqa.exe` SHA-256 `C757803FDD606DD2618FD459A94EEEBF6243D70F4E9DA4C2FE019DD77DF036FA`.
- An earlier run in the same session reported 30 tests and binary `032EBFAD…`. It was stale: restoring `host.rs` after the size measurement kept an older mtime, so cargo reused the test binary that still had the temporary test. After touching the file, the rerun above rebuilt from the candidate source. That stale run is not evidence.
- No new test. The change is structural, and the existing suite has no `Surface` seam; one would add code to a refactor meant to shrink.
- Toolchain: `.tools` in this worktree is a directory junction to the root checkout's pinned toolchain (git-ignored). Nothing was downloaded, and PATH is unchanged.

## Independent review (Opus 5.5 High, clean-code-guard, read-only)

| Round | Manifest SHA-256 | Verdict | Task |
| --- | --- | --- | --- |
| 1 | `13C60C00…F2D237F` | **ACCEPT**, 0 blocker/major/minor, 2 optional nits | <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A09bc86ad-2306-4226-8c8a-23010320204f%3Adelegate-task%3Af0f0c050-892c-4103-8f5e-11b60b729d9c> |

Report: `artifacts/m3-surface-roles-1/round-1/review-opus-1.md`. The reviewer verified all 10 manifest entries and checked that `candidate.diff` matched the live diff. It found no per-wake cost, no behaviour drift, and judged the earlier drop of the `uia` Arc, slot `Vec` and `area` `Vec` harmless.

### Findings and dispositions

- **M3SR-OPUS-NIT-1** (role variant doc comments omit the shadow and UIA root they hold): **deferred**. Comment-only. Fixing it would need a rebuild, a new manifest and a fresh recheck while native idle measurement needs the machine quiet. Fold it into the next `host.rs` candidate.
- **M3SR-OPUS-NIT-2** (`Surface::create` picks the role by shadow presence, which could use a one-line comment): **deferred**, same reason.

## Limits

- Static review only. Native suites, idle/trace measurement, hover timing, Narrator/UIA behaviour and multi-monitor transitions were **NOT RUN** on this binary.
- No council review and no accessibility pass (deferred, not passed). This candidate does not accept any milestone.
