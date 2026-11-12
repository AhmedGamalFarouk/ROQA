# Z1 size trim (2026-11-12)

Branch `feat/z1-trim` from main e8ebe0a. Release exe 1,243,648 B -> 1,121,280 B (-122,368 B, -9.8%). No behaviour, idle or hot-path change.

Method: linker `/MAP` build in a private target dir, symbol sizes from address deltas (`cargo rustc ... -- -C link-arg=/MAP:...`). `.text` was ~1.03 MB; `std::sync::mpsc` was 46 KB (66 symbols).

| Change | Bytes saved |
|---|---|
| `src/chan.rs`: one small bounded queue (`Mutex<Vec>` + `Condvar`, std `mpsc` semantics: FIFO, `send` blocks when full, dropped end disconnects) replaces all `mpsc::{channel, sync_channel}` in brightness, icons, limits_os, weather_os, media, window_catalog, shelf_os. std instantiates its list, array and zero flavors per message type (~5 KB each). Capacities unchanged (8, 64, 1; unbounded where it was `channel()`). | ~122,000 (with the timer sort) |
| `timer.rs`: two `sort_by_key` calls use `sort::by`, dropping the std stable-sort instantiation. | included above (not separated) |

Tests: 360 pass (2 new in `chan.rs`: bounded FIFO and disconnect, blocked send/recv wake across threads). Tests still on std `mpsc`: icons decode and the media `hung` helper (test-only).

Not changed / rejected: `windows` crate features (they gate declarations; no code is pulled by an unused feature under fat LTO); std backtrace and `rustc_demangle` (~30 KB, linked by std's default panic hook; removing needs `build-std`); `BTreeMap`/`BTreeSet` (14 KB, ordered output depends on them); `f64::from_str` (7 KB, used for weather and limits numbers).
Remaining ideas: `sort()`/`dedup` on strings in `prayer_os`, `settings` (one more std sort instantiation, a few KB); `host.rs` `Desktop::update` (44 KB) and `drain_messages` (40 KB) are the largest functions.
NOT RUN: native/GUI (no desktop authorization); thread shutdown, wake and queue-full behaviour covered by unit tests only.
