//! Opt-in redacted diagnostics report (`--diagnostics <path>`). Privacy rule: integers, booleans and fixed
//! words only, never a title, path, track, file or user string. The builder takes numbers (the catalog's
//! window list only contributes handles, process ids and a title *length*), and a test fails if any other
//! string appears in the output. Counters are process statics bumped where the work already happens.
use crate::window_model::{Identity, SourceStats, WindowModel};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
use windows::{
    Win32::{
        Foundation::E_FAIL,
        System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
            Threading::{
                GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources,
                GetProcessHandleCount,
            },
        },
        UI::WindowsAndMessaging::{GetSystemMetrics, SM_CMONITORS},
    },
    core::{Error, Result},
};

const MIB: u64 = 1 << 20;
/// PLAN council gates: private bytes with one monitor and with three.
const ONE_MONITOR_MAX: u64 = 80 * MIB;
const THREE_MONITOR_MAX: u64 = 120 * MIB;
/// PLAN: static CPU at most 1 ms/s over 15 minutes. Only the harness can measure it.
const STATIC_CPU_MS_PER_S: u64 = 1;
const NOT_WIRED: u64 = u64::MAX;

/// Store writes; `NOT_WIRED` (reported as unavailable) until the first `Store::flush` calls `note_store_write`.
static STORE_WRITES: AtomicU64 = AtomicU64::new(NOT_WIRED);
/// Calls that found media work, and every host call of `Media::refresh`.
pub static MEDIA_WAKES: AtomicU64 = AtomicU64::new(0);
pub static MEDIA_REFRESHES: AtomicU64 = AtomicU64::new(0);
/// Cached icons, published by the icon service.
pub static ICON_CACHE: AtomicU64 = AtomicU64::new(0);

pub fn note_store_write() {
    let _ = STORE_WRITES.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
        Some(if n == NOT_WIRED {
            1
        } else {
            n.saturating_add(1)
        })
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Inconclusive,
}
impl Verdict {
    fn word(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Inconclusive => "inconclusive",
        }
    }
}
/// A gate is only judged when it applies and a value was measured; otherwise it is INCONCLUSIVE, never guessed.
pub fn verdict(measured: Option<u64>, limit: u64, applies: bool) -> Verdict {
    match measured {
        Some(value) if applies => {
            if value <= limit {
                Verdict::Pass
            } else {
                Verdict::Fail
            }
        }
        _ => Verdict::Inconclusive,
    }
}

/// Process-wide numbers; `None` means not available (never zero-filled).
#[derive(Clone, Copy, Default, Debug)]
pub struct Counters {
    pub store_writes: Option<u64>,
    pub media_wakes: u64,
    pub media_refreshes: u64,
    pub icon_cache: u64,
    pub icon_cache_cap: u64,
    pub handles: Option<u64>,
    pub gdi: Option<u64>,
    pub user: Option<u64>,
    pub private_bytes: Option<u64>,
    pub monitors: u64,
}
impl Counters {
    pub fn read() -> Self {
        let store = STORE_WRITES.load(Ordering::Relaxed);
        // SAFETY: the pseudo handle needs no close; each out pointer is a live local of the size passed.
        let (handles, gdi, user, private) = unsafe {
            let process = GetCurrentProcess();
            let mut count = 0;
            let handles = GetProcessHandleCount(process, &mut count)
                .is_ok()
                .then_some(u64::from(count));
            // A GUI process always owns some objects, so zero means the call failed.
            let gui = |flag| Some(u64::from(GetGuiResources(process, flag))).filter(|n| *n != 0);
            let mut memory = PROCESS_MEMORY_COUNTERS_EX::default();
            let size = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            let private = GetProcessMemoryInfo(
                process,
                (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                size,
            )
            .is_ok()
            .then_some(memory.PrivateUsage as u64);
            (handles, gui(GR_GDIOBJECTS), gui(GR_USEROBJECTS), private)
        };
        Self {
            store_writes: (store != NOT_WIRED).then_some(store),
            media_wakes: MEDIA_WAKES.load(Ordering::Relaxed),
            media_refreshes: MEDIA_REFRESHES.load(Ordering::Relaxed),
            icon_cache: ICON_CACHE.load(Ordering::Relaxed),
            icon_cache_cap: crate::icons::CACHE_LIMIT as u64,
            handles,
            gdi,
            user,
            private_bytes: private,
            // SAFETY: scalar query.
            monitors: unsafe { GetSystemMetrics(SM_CMONITORS) }.max(0) as u64,
        }
    }
}

/// The catalog's own counters, passed in so this module owns the output format.
pub struct CatalogView<'a> {
    pub model: &'a WindowModel,
    pub stats: SourceStats,
    pub scans: u64,
    pub discarded: u64,
    pub invalidations: u64,
    pub reconciliations: u64,
    pub hooks: usize,
}

fn number(value: Option<u64>) -> String {
    value.map_or_else(|| "null".into(), |n| n.to_string())
}

/// The budget table: (name, limit, measured, applies).
fn budgets(c: &Counters) -> [(&'static str, u64, Option<u64>, bool); 4] {
    [
        (
            "private_bytes_one_monitor",
            ONE_MONITOR_MAX,
            c.private_bytes,
            c.monitors == 1,
        ),
        (
            "private_bytes_three_monitors",
            THREE_MONITOR_MAX,
            c.private_bytes,
            c.monitors >= 3,
        ),
        (
            "icon_cache_entries",
            c.icon_cache_cap,
            Some(c.icon_cache),
            true,
        ),
        // Needs a 15-minute CPU-time delta from the harness; not measurable in-process from one report.
        ("static_cpu_ms_per_s", STATIC_CPU_MS_PER_S, None, true),
    ]
}

pub fn report(catalog: &CatalogView, c: &Counters, stopped: bool) -> String {
    let model = catalog.model;
    // Handles, process ids and a title length only; the title and identity text stay out.
    let windows = model.windows.iter().map(|window| format!(
        "{{\"handle\":{},\"process\":{},\"title_utf16_length\":{},\"identity_available\":{},\"minimized\":{},\"foreground\":{}}}",
        window.key.handle, window.key.process, window.title.encode_utf16().count(),
        !matches!(window.identity, Identity::Unavailable(_)), window.minimized, window.foreground
    )).collect::<Vec<_>>().join(",");
    let rows = budgets(c)
        .iter()
        .map(|(name, limit, measured, applies)| {
            format!(
                "{{\"name\":\"{name}\",\"limit\":{limit},\"measured\":{},\"verdict\":\"{}\"}}",
                number(*measured),
                verdict(*measured, *limit, *applies).word()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"generation\":{},\"complete\":{},\"groups\":{},\"full_scans\":{},\"delta_batches\":{},\"inspected\":{},\"scans\":{},\"discarded\":{},\"invalidations\":{},\"reconciliations\":{},\"hooks_live\":{},\"stopped\":{},\"windows\":[{}],\"counters\":{{\"store_writes\":{},\"media_wakes\":{},\"media_refreshes\":{},\"icon_cache\":{},\"handles\":{},\"gdi\":{},\"user\":{},\"private_bytes\":{},\"monitors\":{}}},\"budgets\":[{}]}}\n",
        model.generation,
        model.incomplete.is_none(),
        model.groups().len(),
        catalog.stats.full_scans,
        catalog.stats.delta_batches,
        catalog.stats.inspected,
        catalog.scans,
        catalog.discarded,
        catalog.invalidations,
        catalog.reconciliations,
        catalog.hooks,
        stopped,
        windows,
        number(c.store_writes),
        c.media_wakes,
        c.media_refreshes,
        c.icon_cache,
        number(c.handles),
        number(c.gdi),
        number(c.user),
        number(c.private_bytes),
        c.monitors,
        rows
    )
}

/// Atomic temp-and-rename write.
pub fn write(path: &Path, output: String) -> Result<()> {
    let temporary = path.with_extension(format!("roqa-{}.tmp", std::process::id()));
    std::fs::write(&temporary, output)
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|error| {
            Error::new(
                E_FAIL,
                format!("Cannot save requested diagnostics: {error}"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_model::{WindowKey, WindowRecord};

    const SECRETS: [&str; 6] = [
        "Secret Title",
        "C:\\Users\\bob\\private.exe",
        "c:\\users\\bob\\private.exe",
        "Some.Aumid!App",
        "Artist - Song",
        "bob",
    ];

    fn model() -> WindowModel {
        let window = |handle, title: &str, identity| WindowRecord {
            key: WindowKey { handle, process: 7 },
            title: title.into(),
            identity,
            executable: Some("c:\\users\\bob\\private.exe".into()),
            minimized: false,
            foreground: true,
        };
        WindowModel {
            generation: 3,
            windows: vec![
                window(
                    10,
                    "Secret Title",
                    Identity::Executable("C:\\Users\\bob\\private.exe".into()),
                ),
                window(
                    11,
                    "Artist - Song",
                    Identity::Application("Some.Aumid!App".into()),
                ),
                window(12, "bob", Identity::Unavailable(7)),
            ],
            incomplete: Some("Secret Title".into()),
        }
    }
    fn counters() -> Counters {
        Counters {
            store_writes: Some(2),
            media_wakes: 5,
            media_refreshes: 9,
            icon_cache: 3,
            icon_cache_cap: 256,
            handles: Some(120),
            gdi: Some(30),
            user: None,
            private_bytes: Some(70 * MIB),
            monitors: 1,
        }
    }
    fn view(model: &WindowModel) -> CatalogView<'_> {
        CatalogView {
            model,
            stats: SourceStats::default(),
            scans: 1,
            discarded: 0,
            invalidations: 2,
            reconciliations: 0,
            hooks: 6,
        }
    }

    #[test]
    fn titles_paths_and_tracks_never_appear() {
        let model = model();
        let output = report(&view(&model), &counters(), false);
        for secret in SECRETS {
            assert!(!output.contains(secret), "leaked {secret}");
        }
    }

    #[test]
    fn only_allowlisted_strings_appear() {
        let model = model();
        let output = report(&view(&model), &counters(), true);
        let allowed = [
            "generation",
            "complete",
            "groups",
            "full_scans",
            "delta_batches",
            "inspected",
            "scans",
            "discarded",
            "invalidations",
            "reconciliations",
            "hooks_live",
            "stopped",
            "windows",
            "handle",
            "process",
            "title_utf16_length",
            "identity_available",
            "minimized",
            "foreground",
            "counters",
            "store_writes",
            "media_wakes",
            "media_refreshes",
            "icon_cache",
            "handles",
            "gdi",
            "user",
            "private_bytes",
            "monitors",
            "budgets",
            "name",
            "limit",
            "measured",
            "verdict",
            "pass",
            "fail",
            "inconclusive",
            "private_bytes_one_monitor",
            "private_bytes_three_monitors",
            "icon_cache_entries",
            "static_cpu_ms_per_s",
        ];
        for (index, token) in output.split('"').enumerate() {
            if index % 2 == 1 {
                assert!(allowed.contains(&token), "unlisted string field {token}");
            }
        }
    }

    #[test]
    fn counters_format_as_integers_or_null() {
        let model = model();
        let output = report(&view(&model), &counters(), false);
        assert!(output.contains("\"counters\":{\"store_writes\":2,\"media_wakes\":5,\"media_refreshes\":9,\"icon_cache\":3,\"handles\":120,\"gdi\":30,\"user\":null,\"private_bytes\":73400320,\"monitors\":1}"));
        let unwired = Counters {
            store_writes: None,
            ..counters()
        };
        assert!(report(&view(&model), &unwired, false).contains("\"store_writes\":null"));
        assert!(output.ends_with("]}\n") && !output.contains('.'));
    }

    #[test]
    fn verdict_never_guesses() {
        assert_eq!(
            verdict(Some(80 * MIB), ONE_MONITOR_MAX, true),
            Verdict::Pass
        );
        assert_eq!(
            verdict(Some(80 * MIB + 1), ONE_MONITOR_MAX, true),
            Verdict::Fail
        );
        assert_eq!(verdict(None, ONE_MONITOR_MAX, true), Verdict::Inconclusive);
        assert_eq!(
            verdict(Some(1), ONE_MONITOR_MAX, false),
            Verdict::Inconclusive
        );
    }

    #[test]
    fn gates_apply_by_monitor_count() {
        let word = |monitors, bytes| {
            let rows = budgets(&Counters {
                monitors,
                private_bytes: Some(bytes),
                icon_cache_cap: 256,
                ..Counters::default()
            });
            rows.map(|(_, limit, measured, applies)| verdict(measured, limit, applies))
        };
        assert_eq!(
            word(1, 90 * MIB)[..2],
            [Verdict::Fail, Verdict::Inconclusive]
        );
        assert_eq!(
            word(2, 90 * MIB)[..2],
            [Verdict::Inconclusive, Verdict::Inconclusive]
        );
        assert_eq!(
            word(3, 110 * MIB)[..2],
            [Verdict::Inconclusive, Verdict::Pass]
        );
        assert_eq!(word(1, 70 * MIB)[3], Verdict::Inconclusive); // CPU is never judged in-process
    }

    #[test]
    fn store_writes_start_unreported_then_count() {
        STORE_WRITES.store(NOT_WIRED, Ordering::Relaxed);
        note_store_write();
        note_store_write();
        assert_eq!(STORE_WRITES.load(Ordering::Relaxed), 2);
    }
}
