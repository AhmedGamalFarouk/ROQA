# Features plan: token-lean execution of FEATURES_TO_ADD.md (2026-11-12)

Rules: one slice at a time, from fresh `main`; one Sonnet 5.5 worker per slice, `/ponytail:ponytail`, brief of at most 12 lines (scope, files, acceptance, "read AGENTS.md"); gate is `scripts/verify.ps1` plus `scripts/land.ps1`, no per-slice review (reviews paused; milestone gates only); no native run without a fresh user confirmation.

Token plan: workers read only files the brief names and use `graft ask`/`graft callers`, not whole-file reads; a slice is a whole feature, not a micro-step; final report is 10 lines. Gemini 3.8 (via `delegate_task`, only if `orchestrator_capabilities` lists it) is for read-only research: the AI-usage data-source check (slice 9) and Windows API surveys. Never for edits or git.

## Already in the tree (do not rebuild; verify against the M5 checklist, not this file)
Media island (GSMTC, art, progress, controls, volume row), pins and drag reorder, timers, file shelf, fullscreen suppression, multi-monitor and DPI, picker window, themes incl. glass, high contrast.

## Queue (size = est. new lines; all event-driven, no new thread, timer or poll)
| # | Slice | Size | Notes / decisions needed |
|---|---|---|---|
| 1 | Global hotkeys (`RegisterHotKey`, not a hook): picker, cycle windows, later audio cycle | S | Opt-in, stored in settings; avoid `Win+Space` (IME switch); needs one always-live HWND owner |
| 2 | System accent colour (`HKCU\...\DWM\AccentColor`, re-read on existing `WM_SETTINGCHANGE`) as optional `Palette.accent` for progress/pills | S | Opt-in so the default look is unchanged; no WinRT |
| 3 | Battery/power pill (`WM_POWERBROADCAST` `PBT_APMPOWERSTATUSCHANGE`, `GetSystemPowerStatus`) | M | Transient pill only; no polling |
| 4 | Audio output switcher + hotkey (Core Audio; `IPolicyConfig` is undocumented) | M | Decide: accept undocumented COM or list-only |
| 5 | Dock context menu (close, new instance, pin/unpin) | M | `TrackPopupMenu`; admin launch is elevation: out unless approved |
| 6 | Mica/Acrylic backdrop, dock sizing options | M | Touches DComp surfaces: needs native visual check |
| 7 | DWM thumbnails on hover | L | Per-hover resource cost; needs budget first |
| 8 | Virtual desktop switcher | M | `IVirtualDesktopManager` only reads and moves windows; switching is undocumented API |
| 9 | AI subscription usage meter | L | Verify each provider's data source first (Gemini research task); credentials via Credential Manager only |

## Blocked or deferred by project rules
- Caps/Num Lock indicator: needs a global hook or polling (both forbidden). Defer.
- Privacy indicators (mic/camera): needs polling of consent registry or a hook. Defer until a cheap event source is found.
- Volume/brightness OSD replacement: needs media-key interception (hook). Defer.
- Run as administrator, jump lists, notification badges, command palette, dock magnification: each needs its own design note first.

## Next
Slice 1 and 2 are the cheapest, both opt-in and unobservable by default. Start with slice 1 once the hotkey defaults are chosen.
