# Panic abort 1: `panic = "abort"` in release

Base `5655f23`, pinned toolchain, `cargo build --locked --release`. No native/GUI run.

| | roqa.exe B | SHA-256 |
|---|---:|---|
| before (fat LTO) | 1,048,576 | `92FC18C188D0DF9657710E8AC907D6C5B31C4C362FDBB7DD0FBE28B4F95512D3` |
| after | 803,840 (-244,736, -23.3%) | `D7592F2D06AE878377EB78F49B064AAA99E507D36578A1A24E47855D75029AC2` |

footprint-1 measured 795,136 B without a hook; the hook, its format and `TerminateProcess` cost about 8.7 KB.

**Hook** (`error_log::install_panic_hook`, first line of `main`, all profiles): writes `ROQA panic at <file>:<line> on <thread>` with `try_lock`, then `TerminateProcess(GetCurrentProcess(), 70)`. The payload is never logged (it can carry titles or paths). The line is dropped if the log is not open (before admission, or a non-host launch) or locked; the exit code is still 70. Exit codes: 70 panic, 1 startup failure, 0 normal; the debug `--probe-panic` now exits 70 (was 1 via `PostQuitMessage`), so `smoke.ps1` expects 70 for it.

**Sites** (`catch_unwind` removed everywhere; tests keep unwind, `verify.ps1` 185 tests, +1 for the line format):
- `native_window.rs` wndproc: calls `dispatch` directly; the hook replaces `PostQuitMessage(70)`.
- `window_source.rs` enumerate: the guarded body (flag, bound check, `Vec::push`) has no panic path; plain returns.
- `picker.rs` dialog_proc: the `Result` handling stays (`PostMessageW` failure still posts quit 71); no panic wrapper.
- `shelf_os.rs` picker thread: `pick_files` already returns an empty `Vec` for every error and has no `unwrap`/`expect`; its COM-init error path is unchanged.

**Worker-path audit** (icon worker, window-source worker, file picker, `window_source` scan). No `unwrap`/`expect`/`panic!`/`unreachable!`/`RefCell` in non-test worker code. Remaining sites, each justified:
- `window_source.rs:148,157,216` `&buf[..n]`: `n` is the API's returned count (`GetClassNameW`, `GetWindowTextW`, `QueryFullProcessImageNameW`), at most the buffer length by contract. Left as is.
- `shelf_os.rs:72` `&buffer[..copied.min(length)]`: buffer is `length + 1`. `shelf_os.rs:319-322` `bytes[0..1]`: guarded by `len() < 3`.
- `icons.rs:325-326` `width - square`: `square = min(width, height)`. `:306` size: `side <= ART_SIDE`; `:364` size: bitmap sides checked 1..=512. `premultiply` `p[3]`, `p[..3]` on `chunks_exact(4)`; `/ 255`, `/ 2` constant divisors.
- `window_catalog.rs:65` `HANDLES[slot]`: guarded by `slot < RING_CAPACITY` (WinEvent callback, UI thread).
- `media.rs` album-art `done` closure (runs on the icon worker): `OnceLock::set` result ignored, `raise` does not panic.
- `join().is_err()` at `icons.rs:244` and `window_catalog.rs:338` is now unreachable in release (kept: still true in unwind tests).
- The shelf drop target's `RefCell::borrow_mut` (`shelf_os.rs:116-157`) is an OLE callback on the UI thread; it was never guarded, so abort is not worse.
- Allocation failure aborts in both profiles.

**NOT RUN:** a real crash (no forced panic in the release binary), the actual `%LOCALAPPDATA%\ROQA\error.log` line, exit code 70 observed, the debug smoke with `--probe-panic`, and all native/GUI/performance gates. The hook's line format is unit-tested only.

**Known limit: unsaved edits.** `TerminateProcess` skips `flush_store(true)`, so pin, shelf, timer and settings edits still inside the store write delay (up to `MAX_WAIT`, 10 s) are lost when the host panics. Flushing from the hook is not safe (it may run while the store or a lock is in use), so the limit is accepted. Found by the M5 gate 1 review (M5).
