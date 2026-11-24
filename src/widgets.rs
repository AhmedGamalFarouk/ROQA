//! Dock widget readings and preferences.
use std::{
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetPreferences {
    /// Network speed in bits per second.
    pub net_bits: bool,
    /// The Disk tile's drive letter (upper case); `None` is the system drive.
    pub disk_drive: Option<u8>,
    /// Battery percent at or under which the tile and the pill warn (0: only Windows' own low flag).
    pub low_at: u8,
    /// The week strip starts on Sunday.
    pub week_sunday: bool,
    /// A bolt in the closed pill while the battery is low.
    pub pill_low: bool,
}

impl Default for WidgetPreferences {
    fn default() -> Self {
        Self {
            net_bits: false,
            disk_drive: None,
            low_at: 20,
            week_sunday: false,
            pill_low: true,
        }
    }
}

/// How often the system tiles are read while they show.
pub const STATS_STEP: Duration = Duration::from_secs(1);

/// CPU in use and network speed: differences between two samples `STATS_STEP` apart, taken only while an
/// open dock popup shows the tiles (`Desktop::refresh_stats`); dropped when it closes.
#[derive(Clone, Copy, Default)]
pub struct Stats {
    last: Option<Sample>,
    pub cpu: Option<u32>,
    /// Bytes per second down and up.
    pub net: Option<(u64, u64)>,
}

#[derive(Clone, Copy)]
struct Sample {
    at: Instant,
    /// Idle and total processor time (100 ns units).
    cpu: Option<(u64, u64)>,
    /// Bytes received and sent by the physical adapters.
    net: Option<(u64, u64)>,
}

impl Stats {
    /// Reads once the step is due; true when it did (the tiles repaint).
    pub fn sample(&mut self, now: Instant) -> bool {
        if self.deadline().is_some_and(|due| now < due) {
            return false;
        }
        let next = Sample {
            at: now,
            cpu: cpu_times(),
            net: net_bytes(),
        };
        if let Some(last) = self.last {
            (self.cpu, self.net) = rates(last, next);
        }
        self.last = Some(next);
        true
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.last.map(|sample| sample.at + STATS_STEP)
    }
}

#[derive(Default)]
pub struct SystemSample {
    pub counters: Stats,
    pub memory: Option<Usage>,
    pub disk: Option<Usage>,
}

type Answer = (Instant, SystemSample);
static SYSTEM_ANSWER: Mutex<Option<Answer>> = Mutex::new(None);
static SYSTEM_BUSY: AtomicBool = AtomicBool::new(false);
const SYSTEM_READY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 121;

pub fn request_system(opened: Instant, mut counters: Stats, drive: Option<u8>) -> bool {
    if SYSTEM_BUSY.swap(true, Ordering::AcqRel) {
        return true;
    }
    // SAFETY: reads the host thread ID without pointers.
    let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let sent = std::thread::Builder::new()
        .name("roqa-system-reading".into())
        .spawn(move || {
            counters.sample(Instant::now());
            let sample = SystemSample {
                counters,
                memory: memory(),
                disk: disk(drive),
            };
            *SYSTEM_ANSWER.lock().unwrap_or_else(PoisonError::into_inner) = Some((opened, sample));
            SYSTEM_BUSY.store(false, Ordering::Release);
            let _ = crate::native_window::post_wake(host, SYSTEM_READY);
        })
        .is_ok();
    if !sent {
        SYSTEM_BUSY.store(false, Ordering::Release);
    }
    sent
}

pub fn system_answer() -> Option<Answer> {
    SYSTEM_ANSWER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

fn rates(a: Sample, b: Sample) -> (Option<u32>, Option<(u64, u64)>) {
    let cpu = a
        .cpu
        .zip(b.cpu)
        .and_then(|((idle0, total0), (idle1, total1))| {
            let total = total1.saturating_sub(total0);
            let busy = total.saturating_sub(idle1.saturating_sub(idle0));
            (total > 0).then(|| (busy * 100 / total).min(100) as u32)
        });
    let seconds = b.at.saturating_duration_since(a.at).as_secs_f64();
    let net = a
        .net
        .zip(b.net)
        .filter(|_| seconds > 0.0)
        .map(|((down0, up0), (down1, up1))| {
            let per_second = |from: u64, to: u64| (to.saturating_sub(from) as f64 / seconds) as u64;
            (per_second(down0, down1), per_second(up0, up1))
        });
    (cpu, net)
}

fn cpu_times() -> Option<(u64, u64)> {
    use windows::Win32::{Foundation::FILETIME, System::Threading::GetSystemTimes};
    let (mut idle, mut kernel, mut user) = Default::default();
    // SAFETY: three live out values.
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    let ticks = |t: FILETIME| u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime);
    // Kernel time includes the idle time.
    Some((ticks(idle), ticks(kernel) + ticks(user)))
}

fn net_bytes() -> Option<(u64, u64)> {
    use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    // SAFETY: the API allocates the table; it is read only while alive and freed once below.
    unsafe {
        GetIfTable2(&mut table).ok().ok()?;
        let rows =
            std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        // Physical adapters only (hardware, not a filter layer), so one transfer is counted once.
        let total = rows
            .iter()
            .filter(|row| row.InterfaceAndOperStatusFlags._bitfield & 0b11 == 0b01)
            .fold((0u64, 0u64), |(down, up), row| {
                (down + row.InOctets, up + row.OutOctets)
            });
        FreeMibTable(table.cast());
        Some(total)
    }
}

/// "2.4 MB/s", or "19 Mb/s" in bits: a speed in the units the tile shows.
pub fn speed(bytes_per_second: u64, bits: bool) -> String {
    let (unit, value) = if bits {
        ('b', bytes_per_second as f64 * 8.0 / 1000.0)
    } else {
        ('B', bytes_per_second as f64 / 1024.0)
    };
    let step = if bits { 1000.0 } else { 1024.0 };
    if value < 1000.0 {
        format!("{value:.0} K{unit}/s")
    } else if value < 1000.0 * step {
        format!("{:.1} M{unit}/s", value / step)
    } else {
        format!("{:.1} G{unit}/s", value / step / step)
    }
}

/// A usage tile's reading: percent in use and one line of detail.
pub struct Usage {
    pub percent: u32,
    pub detail: String,
}

fn gigabytes(bytes: u64) -> f64 {
    bytes as f64 / f64::from(1u32 << 30)
}

/// "6.2 of 16 GB" and the percent it is, or `None` for an empty total.
fn usage(used: u64, total: u64) -> Option<Usage> {
    (total > 0).then(|| Usage {
        percent: (used * 100 / total).min(100) as u32,
        detail: format!("{:.1} of {:.0} GB", gigabytes(used), gigabytes(total)),
    })
}

/// Physical memory in use, read when the tile is drawn (no timer).
pub fn memory() -> Option<Usage> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `status` is a live, sized out value.
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    usage(
        status.ullTotalPhys - status.ullAvailPhys,
        status.ullTotalPhys,
    )
}

/// The fixed drives' letters, read when Settings lists them.
pub fn fixed_drives() -> Vec<u8> {
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    use windows::core::HSTRING;
    // SAFETY: no arguments.
    let mask = unsafe { GetLogicalDrives() };
    (b'A'..=b'Z')
        .filter(|letter| mask >> (letter - b'A') & 1 == 1)
        // SAFETY: the root string outlives the call. 3 is DRIVE_FIXED.
        .filter(|&letter| unsafe { GetDriveTypeW(&HSTRING::from(format!("{}:\\", letter as char))) } == 3)
        .collect()
}

/// A drive's space in use (the chosen one, else the system drive), read when the tile is drawn.
pub fn disk(letter: Option<u8>) -> Option<Usage> {
    use windows::{Win32::Storage::FileSystem::GetDiskFreeSpaceExW, core::HSTRING};
    let drive = letter.map_or_else(
        || std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()),
        |letter| format!("{}:", letter as char),
    );
    let root = HSTRING::from(format!("{drive}\\"));
    let (mut free, mut total) = (0u64, 0u64);
    // SAFETY: `root` and both out values outlive the call.
    unsafe { GetDiskFreeSpaceExW(&root, Some(&mut free), Some(&mut total), None) }.ok()?;
    usage(total.saturating_sub(free), total)
}

/// Monday-first day numbers for the week containing a valid local date.
pub fn week_numbers(year: u16, month: u16, day: u16, sunday_first_weekday: u16) -> [u16; 7] {
    let today = (sunday_first_weekday + 6) % 7;
    let previous_month = if month == 1 { 12 } else { month - 1 };
    let previous_year = if month == 1 { year - 1 } else { year };
    let previous_days = month_days(previous_year, previous_month);
    let current_days = month_days(year, month);
    std::array::from_fn(|index| {
        let number = i32::from(day) + index as i32 - i32::from(today);
        if number < 1 {
            (number + i32::from(previous_days)) as u16
        } else if number > i32::from(current_days) {
            (number - i32::from(current_days)) as u16
        } else {
            number as u16
        }
    })
}

pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Six weeks of a month, 42 cells in reading order: (day number, in this month). The first row is
/// `week_numbers` of the 1st, so the leading days come from the previous month; the tail runs into the next.
pub fn month_grid(year: u16, month: u16, sunday_first: bool) -> [(u16, bool); 42] {
    // Days since 1970-01-01 (a Thursday) to a Sunday-based weekday of the 1st.
    let first = (crate::prayer_tab::day_number((year.into(), month.into(), 1)) + 4).rem_euclid(7);
    let row = week_numbers(year, month, 1, (first as u16 + u16::from(sunday_first)) % 7);
    let lead = row.iter().position(|&day| day == 1).unwrap_or(0);
    let days = month_days(year, month);
    std::array::from_fn(|cell| match cell {
        0..7 => (row[cell], cell >= lead),
        _ => match (cell - lead + 1) as u16 {
            day if day <= days => (day, true),
            day => (day - days, false),
        },
    })
}

fn month_days(year: u16, month: u16) -> u16 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_reads_percent_and_gigabytes() {
        let gb = 1u64 << 30;
        let reading = usage(4 * gb, 16 * gb).unwrap();
        assert_eq!(
            (reading.percent, reading.detail.as_str()),
            (25, "4.0 of 16 GB")
        );
        assert!(usage(0, 0).is_none());
        assert_eq!(usage(20, 10).unwrap().percent, 100);
    }

    #[test]
    fn week_crosses_month_year_and_leap_boundaries() {
        assert_eq!(week_numbers(2024, 3, 1, 5), [26, 27, 28, 29, 1, 2, 3]);
        assert_eq!(week_numbers(2023, 1, 1, 0), [26, 27, 28, 29, 30, 31, 1]);
        assert_eq!(week_numbers(2023, 12, 31, 0), [25, 26, 27, 28, 29, 30, 31]);
        assert_eq!(week_numbers(2024, 12, 31, 2), [30, 31, 1, 2, 3, 4, 5]);
        assert_eq!(week_numbers(2023, 5, 31, 3), [29, 30, 31, 1, 2, 3, 4]);
        assert_eq!(week_numbers(2100, 3, 1, 1), [1, 2, 3, 4, 5, 6, 7]);
        for weekday in 0..7 {
            let days = week_numbers(2024, 6, 15, weekday);
            assert_eq!(days[usize::from((weekday + 6) % 7)], 15);
        }
        assert_eq!(month_days(2000, 2), 29);
        assert_eq!(month_days(2100, 2), 28);
    }

    #[test]
    fn month_grid_places_the_first_and_spills_into_neighbours() {
        // November 2026 starts on a Sunday; February 2024 on a Thursday.
        let nov = month_grid(2026, 11, false);
        assert_eq!(
            (nov[5], nov[6], nov[7]),
            ((31, false), (1, true), (2, true))
        );
        assert_eq!(
            (nov[35], nov[36], nov[37]),
            ((30, true), (1, false), (2, false))
        );
        let sunday = month_grid(2026, 11, true);
        assert_eq!(
            (sunday[0], sunday[29], sunday[30]),
            ((1, true), (30, true), (1, false))
        );
        let feb = month_grid(2024, 2, false);
        assert_eq!(
            (feb[2], feb[3], feb[31], feb[32]),
            ((31, false), (1, true), (29, true), (1, false))
        );
        assert_eq!(month_grid(2023, 1, true)[0], (1, true));
    }

    #[test]
    fn stats_read_rates_between_two_samples() {
        let at = Instant::now();
        let a = Sample {
            at,
            cpu: Some((100, 1000)),
            net: Some((0, 0)),
        };
        let b = Sample {
            at: at + Duration::from_secs(2),
            cpu: Some((175, 1100)),
            net: Some((4096, 2048)),
        };
        assert_eq!(rates(a, b), (Some(25), Some((2048, 1024))));
        // A missing or reset counter reads nothing rather than a wild number.
        assert_eq!(
            rates(
                a,
                Sample {
                    cpu: None,
                    net: None,
                    ..b
                }
            ),
            (None, None)
        );
        assert_eq!(rates(b, a).0, None);
        assert_eq!(speed(512 * 1024, false), "512 KB/s");
        assert_eq!(speed(2 * 1024 * 1024 + 400 * 1024, false), "2.4 MB/s");
        assert_eq!(speed(125_000, true), "1.0 Mb/s");
        // Due once a step has passed; the first sample has no rates yet.
        let mut stats = Stats::default();
        assert!(stats.sample(at) && stats.cpu.is_none());
        assert!(!stats.sample(at + Duration::from_millis(500)));
        assert_eq!(stats.deadline(), Some(at + STATS_STEP));
    }
}
