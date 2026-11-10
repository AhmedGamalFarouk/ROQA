# M3 first slice: monitor topology

Prepared 2026-11-10, primary Codex agent (gpt-6.1-sol, medium). Preparation only, no production edits or runtime results. User deferred accessibility checks and milestone council. Neither deferral is a passed gate or a full M2 acceptance.

## Current coupling

- src/host.rs Desktop stores one work area/DPI; show, notch/dock lookup, placement and display refresh are primary-monitor oriented.
- src/native_window.rs exposes primary_work_area/primary_dpi and forwards display/DPI/settings messages.
- src/layout.rs placement already accepts work bounds, DPI, surface kind and slot count; extend model coverage instead of duplicating geometry.
- Keep one catalog/worker shared across surfaces, not one per monitor. Picker and existing user control routes must remain usable.

## Bounded implementation sequence

1. Introduce a monitor snapshot model and deterministic surface reconciliation decisions separately from Win32 mutation. Cover negative origins, work-area insets, DPI changes, monitor reorder/removal, unchanged snapshots and empty/transient observations.
2. Verify the Windows/Rust enumeration and DPI contracts before wiring platform code. Define session-local identity, retry behavior and callback ownership; never assume enumeration order is identity or that a handle survives topology changes.
3. Key surfaces by monitor identity plus kind. Replace primary-only lookup/placement with explicit monitor ownership while retaining one shared application model. Define existing show/hide commands to apply consistently; preserve primary-monitor picker behavior initially.
4. Reconcile topology on the existing host thread with bounded refresh, explicit cleanup and recoverable creation failure. Reuse retained surfaces where appropriate; removed monitor HWNDs/shadows/UIA providers must be released. Do not temporarily expand input regions or reserve work areas.
5. Require fresh independent non-author clean-code-guard for nontrivial production changes. Council remains deferred. Targeted offline checks can run without GUI; native topology runs need fresh desktop confirmation and available hardware.

## Later slices, not included

Per-monitor fullscreen suppression, on-demand DWM previews, bounded preview caches, resume/Explorer restart and device-loss recovery need their own implementation and runtime cases. Do not hide those requirements inside an unbounded first patch.

## Evidence contract

Deterministic model tests do not prove actual monitor enumeration, mixed-DPI placement or hotplug. Real hardware evidence must cover monitor identity/geometry, negative-coordinate layout, add/remove/reorder, taskbar/work-area preservation, failure cleanup and per-monitor resource cost. No display/settings mutation without authorization. No performance advantage claimed from earlier short idle samples. Preserve previously recorded M2/Windows10/IME/text-scale/compositor limits; no new freeze or milestone acceptance here.
