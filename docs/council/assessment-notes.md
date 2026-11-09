# Chair's evidence checks

Date: 2026-11-09. These notes qualify the independent reports; they are not benchmark results.

## Corrections to carry into the final decision

1. Nectar's **217 MB private commit** is one project-reported observation on five monitors at commit `ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a`, not a structural minimum, independently reproduced result, or universal baseline. Its document also reports reduced renderer counts, so neither one-renderer-per-window nor a particular process count should be treated as an invariant of WebView2. A proposed one-monitor 40 MB prototype cannot be divided into that five-monitor result to claim a speedup. Run the same monitor topology, features, effects and workload for both products.
2. The memory document discusses trimming but also says the saver is off by default. This is a reproducibility question, not proof the author manipulated a particular result. Working-set trimming can lower resident pages without lowering private committed allocations; memory pressure can turn subsequent accesses into expensive faults. Measure residency, commit and responsiveness together. Do not use periodic trimming as the architecture.
3. Compositor animation reduces application-thread work; it does not eliminate DWM/GPU work or presentation. A collapsed, static music notch should have neither application render ticks nor a hidden progress animation. A visible moving progress bar necessarily changes displayed content. Never call all of these states 'zero CPU/GPU'.
4. Prefer OS events where they are reliable and available. Not every feature has a supported event: external-monitor brightness, hardware telemetry, network feeds, and recovery from missed notifications may need bounded polling or occasional reconciliation. Demand-driven polling, deadlines, backoff and cancellation are legitimate. A blanket ban on every timeout is counterproductive.
5. Store policy 10.2.8 requires supported methods and consent. It creates a serious risk for unsupported Explorer/taskbar manipulation, not a categorical ban on every product called a dock or taskbar replacement. Certification of a particular implementation is unverified. Do not assume Nusa's Store presence establishes why it was accepted.
6. A manifest capability and permission are distinct from Store distribution. Full signed sideloaded MSIX is a candidate for notification access. Test `UserNotificationListener` under the actual app model; external-location/sparse identity support remains an explicit unknown. Do not assume a separate Store codebase is required before this probe.
7. Reading GPL source alone does not make independently written software a derivative. Copying, translation and adaptation can carry license obligations. Decide licensing before reuse; do not treat the reviewers' source inspection as automatically disqualifying them from implementation. No source published for Nusa was found in this inspection, but this is not a claim of knowledge of its private implementation or all legal rights.
8. Rust and WinUI are candidates, not measured winners. Prototype composition interop, shaped-window hit testing, accessibility, Arabic/RTL, IME and mixed DPI before choosing. Built-in framework controls help accessibility and text handling; they do not make accessible application semantics automatic. NativeAOT support exists for WinUI, but dependencies and the exact release configuration must be checked.
9. Fullscreen suppression needs per-monitor policy; a system-level notification-state query is supporting evidence, not a complete per-monitor answer. Foreground changes alone also miss geometry changes. Use narrowly filtered move/size/location and display events with coalescing and lifecycle cleanup.
10. Dock activation must respect Windows foreground restrictions. Test normal, elevated, minimized, UWP/packaged, grouped and single-instance apps; report blocked activation rather than repeatedly attempting to steal focus. Default to graceful close; force termination is an explicit action with possible unsaved-work loss.
11. A watchdog that is not running cannot restore the taskbar two seconds after an arbitrary process kill. Immediate recovery in an opt-in replacement mode requires an independently surviving helper or another tested mechanism. Crash handlers do not run on every termination. Account for helper CPU and memory in that mode's totals.

## Primary sources checked by the chair

- [Nectar memory record at reviewed commit](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/docs/memory.md)
- [Microsoft Store policies, section 10.2.8](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies#102-security)
- [Notification listener requirements and access revocation](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/notification-listener)
- [Microsoft NativeAOT announcement for Windows App SDK 1.6](https://blogs.windows.com/windowsdeveloper/2024/09/04/whats-new-in-windows-app-sdk-1-6/)
- [SetForegroundWindow restrictions](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow)

No application was built or benchmarked for this review.
