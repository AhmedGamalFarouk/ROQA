<img src="assets/icon.svg" width="64" height="64" alt="">

# ROQA

A native Windows dock and notch, built in Rust. Development follows the independently reviewed [plan](PLAN.md).

M1 has passed both independent council reviews. The visible UI remains a **graphics and lifecycle proof**, with two small solid capsules and hover feedback. Current M2a development adds the real event-driven window catalog underneath; launcher controls and media are still being built. The Windows taskbar stays available; ROQA makes no startup, registry or work-area changes.

## Develop on Windows 11 x64

Requires the installed Visual Studio 2022 C++ Build Tools and Windows SDK. The setup installs Rust 1.91.1 only inside this project's ignored `.tools` directory; it does not change global PATH. Its archived installer is checksum-verified. The active rustup manager identity and an uninstall-registry observation with unknown attribution are recorded in the candidate packets; PATH suppression is not a claim of complete installer isolation.

```powershell
./scripts/bootstrap.ps1
./scripts/verify.ps1
./scripts/run.ps1
./scripts/run.ps1 quit
```

Use `show-notch`, `hide-notch`, `show-dock` and `hide-dock` with `run.ps1` to create/destroy either surface while the other remains alive. Closing the last surface exits. The raw binary supports `--quit` and corresponding `--show-*`/`--hide-*` commands. Control commands acknowledge delivery; runtime tests separately verify their effects.

Cargo is available through `scripts/cargo.ps1`. In PowerShell, quote the argument separator or pass an explicit array:

```powershell
./scripts/cargo.ps1 clippy --locked --all-targets '--' -D warnings
```

## Verify the native shell

```powershell
./scripts/smoke.ps1
./scripts/measure-idle.ps1
./scripts/catalog-smoke.ps1
```

Run these sequentially on an unlocked interactive desktop with no existing ROQA instance. The smoke test temporarily creates a foreground test window and moves the pointer, then restores them. It moves only test-owned ROQA windows away from screen edges during pixel assertions to avoid interference from other notch/dock overlays. It never closes other applications. All three native scripts accept `-EvidenceDirectory` to keep runs separate. Smoke/idle default to ignored `artifacts/m1/`; the catalog test defaults to `artifacts/m2a/`. `verify.ps1` captures separate check/build stdout, stderr and explicit exit codes under `artifacts/verification/`.

The smoke test checks visible composed pixels and hover/leave behavior, intended placement, focus preservation, work-area preservation, synthetic DPI messages, independent window lifetime, 100 in-process resource cycles, focus-independent quit, last-window exit and failure paths. Physical mixed-DPI changes, media, accessible dock controls, RTL/IME and device-loss recovery belong to later reviewed milestones.

The idle sampler records one short process-counter observation. It excludes DWM/GPU/power attribution and establishes no performance advantage over Nectar or Nusa.

## Window catalog foundation (M2a candidate)

One worker maintains the catalog for both future surfaces. Six narrow out-of-context window-event subscriptions feed a bounded buffer. Ordinary events refresh affected HWNDs after a 50 ms batching interval. Full enumeration handles startup, overflow and a 30-second monotonic reconciliation deadline. These are window-state notifications, with no input interception or taskbar mutation. The main loop checks elapsed deadlines on every bounded pass and limits its blocking wait to the next deadline; it does not depend on WM_TIMER delivery. Incomplete scans retry after 1, 2, 4, 8, 16 and at most 30 seconds, retaining the last complete model. Subscriptions and the worker are explicitly stopped during shutdown.

The catalog test creates real native fixture windows and exercises enumeration, rename/Arabic title metadata, minimize/restore, foreground, hide/show, create/destroy churn, title-event bursts, queue-capacity posted-message traffic, timed reconciliation and shutdown under continuing events. Reading Arabic title metadata is not proof of Arabic rendering or IME support. Those, icons, packaged-app grouping, activation, keyboard and UIA remain in the M2b interaction candidate and the full M2 stack gate.

For explicit numeric diagnostics, start the raw binary with `--diagnostics <output-file>` in an existing directory. This writes catalog counts, HWND/PID identities and state flags, with no titles or executable paths. Ordinary launches write no catalog file. Diagnostics are replaced as snapshots arrive and record successful hook/worker shutdown. Avoid pointing an external reader at the file with a deny-delete sharing mode, since writes replace it atomically.

## Window picker (M2b1 candidate)

Clicking the dock's first ("All windows") slot, or running `./scripts/run.ps1 show-picker`, opens a modeless standard-control window picker built from the live catalog: a native filter edit, list box, status text and Open/Close buttons. Enter or double-click activates the selected real window; Escape or Close dismisses it and returns focus to the previous app when ROQA still owns the foreground. Windows may deny foreground to a background launch; the picker then says so instead of forcing focus. The list is never mock data.

## Dock and notch (M2b)

The dock shows one slot per real window group after the picker slot. Groups use the window's AppUserModelID (packaged apps such as Calculator and Settings stay separate even though one host process owns their frames) and fall back to the executable. Icons come from the shell on a bounded background thread. A click activates the group, cycles through a focused group's windows, or minimizes a lone focused window. Windows refuses minimizing an elevated window from ROQA (UIPI), and the dock gives no feedback. Windows' "Not Responding" ghost windows are not listed separately. The dock exposes a UI Automation toolbar with one invokable button per slot; it is not keyboard-focusable, so the picker is the keyboard and Narrator route. The black notch at the top shows the time and expands to the date on hover.

```powershell
./scripts/packaged-hung-check.ps1
```

This opens Calculator, Settings and a deliberately hung test window (no mouse or keyboard injection), checks separate packaged groups, icons, the hung window and `--quit`, then closes only what it opened.

```powershell
./scripts/picker-smoke.ps1
```

The picker smoke uses a real Arabic-titled fixture window, native UIA reads through `roqa-uia-probe.exe`, keyboard routes, live title updates, activation and 35 open/close cycles with resource sampling. Narrator speech, OS text-scale setting changes and real IME composition are untested.

## Review discipline

[Project rules](AGENTS.md) require independent GPT-6.1 Sol High and Opus 5.5 High review. Authors cannot approve their own work. Each accepted milestone needs both dispositions against the same frozen manifest plus its required evidence. See the [development ledger](docs/development/progress.md) for actual status; a build succeeding is not milestone acceptance.

No source or artwork from Nectar or Nusa is incorporated. The project's release license remains undecided; do not infer a license from dependencies or inspiration.
