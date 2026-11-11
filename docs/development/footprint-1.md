# Footprint 1: release binary size

Base `8c7cf18`, pinned toolchain, `cargo build --locked --release`. Runtime/GUI measurement NOT RUN. Baseline here 1,082,368 B (progress.md's 1,081,856 B differs by path-dependent panic strings).

| Profile change (each built alone) | roqa.exe B | vs base | Decision |
|---|---:|---:|---|
| baseline (thin LTO, strip=debuginfo) | 1,082,368 | | |
| `lto = "fat"` | 1,048,576 | -33,792 (-3.1%) | **adopted**, no hot-path cost |
| `strip = "symbols"` | 1,082,368 | 0 | no effect on MSVC (PDB already separate); same with fat |
| `panic = "abort"` | 795,136 | -287,232 (-26.5%) | **not adopted**, see below |
| `opt-level = "s"` | 865,792 | -216,576 | measure only, hot paths not profiled |
| `opt-level = "z"` | 816,128 | -266,240 | measure only, hot paths not profiled |

`catch_unwind` is relied on at 4 sites: `native_window.rs:389` (wndproc panic becomes `PostQuitMessage(70)`), `window_source.rs:43`, `picker.rs:494`, `shelf_os.rs:429` (picker thread falls back to empty). Abort would turn each into a process abort.

**Features:** removed alone with `cargo check --all-targets`, 9 were redundant (implied by features that stay): `Foundation`, `Win32_Media_Audio`, `Win32_UI_Shell`, `Win32_Foundation`, `Win32_Graphics_Direct2D`, `Win32_Graphics_Dxgi`, `Win32_Security`, `Win32_System_Com`, `Win32_System_Memory`. Removed together, the check passes with the resolved feature set unchanged (hygiene only, no size change). The rest are needed.

**Final (fat LTO):** 1,048,576 B, SHA `5E2744A6A9C5ED5A706B2B0669B843686AA65202CF78ABF44BAF7649E537028B`, verify.ps1 PASS, 184 tests. Net -33,792 B (-3.1%).

## Where the bytes are (MSVC /MAP, fat LTO, .text 804,864 B)

| B | Contributor |
|---:|---|
| 361,543 | `roqa` own code |
| 136,660 | EH cleanup/catch funclets (`dtor$`/`catch$`), all crates; what `panic=abort` removes |
| 105,796 | `core` |
| 73,012 | `std` (backtrace print, mpmc, panic runtime) |
| 51,041 | generic trait impls |
| 31,530 / 26,864 | `alloc` / `windows` (+ `windows_core` 5,392) |

Top functions: `host::Desktop::update` 34,976; `host::drain_messages` 30,080; `window_catalog::Catalog::create` closure 26,800; `host::Surface::paint_notch` 25,504; `icons::IconService::create` closure 17,072; `Desktop::create` 16,304; `Media::refresh` 14,912; `host::event_loop` 13,008.

## Cheap code wins
None applied: `windows` is the only dependency, `{:?}` is in tests or one log line, no big tables (`assets/` is 302 B).

## Deferred
1. `panic = "abort"` (-287 KB): replace the 4 guards with a panic hook that logs and exits 70; needs review and a native pass.
2. `opt-level = "s"` for cold modules only (`window_catalog`, `settings_window`, diagnostics) after profiling.
3. Outline cold paths of `Desktop::update`, `drain_messages`, `Catalog::create` (about 90 KB).
4. Nightly `-Zbuild-std` + `panic_immediate_abort` would drop backtrace/demangle (~25 KB).
