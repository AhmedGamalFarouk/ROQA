//! Configuration for the two expanded-notch widget slots, and the readings of the system tiles.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    WeekCalendar,
    Battery,
    /// Memory in use.
    Memory,
    /// The system drive's space in use.
    Disk,
    /// Processor in use.
    Cpu,
    /// Download and upload speed.
    Network,
    /// The next prayer and the wait for it (the Prayer tab's reading).
    NextPrayer,
    /// Glasses of water drunk today.
    Water,
    /// The temperature and condition (the Weather tab's reading).
    Weather,
}

impl WidgetKind {
    pub const ALL: [Self; 9] = [
        Self::WeekCalendar,
        Self::Battery,
        Self::Memory,
        Self::Disk,
        Self::Cpu,
        Self::Network,
        Self::NextPrayer,
        Self::Water,
        Self::Weather,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::WeekCalendar => "week",
            Self::Battery => "battery",
            Self::Memory => "memory",
            Self::Disk => "disk",
            Self::Cpu => "cpu",
            Self::Network => "network",
            Self::NextPrayer => "prayer",
            Self::Water => "water",
            Self::Weather => "weather",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "week" => Some(Self::WeekCalendar),
            "battery" => Some(Self::Battery),
            "memory" => Some(Self::Memory),
            "disk" => Some(Self::Disk),
            "cpu" => Some(Self::Cpu),
            "network" => Some(Self::Network),
            "prayer" => Some(Self::NextPrayer),
            "water" => Some(Self::Water),
            "weather" => Some(Self::Weather),
            _ => None,
        }
    }
}

/// The Widgets page's choices: two tiles, or four when a lower slot is filled (`None` is empty).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetConfig {
    pub slot_left: WidgetKind,
    pub slot_right: WidgetKind,
    pub slot_left2: Option<WidgetKind>,
    pub slot_right2: Option<WidgetKind>,
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

impl Default for WidgetConfig {
    fn default() -> Self {
        Self {
            slot_left: WidgetKind::WeekCalendar,
            slot_right: WidgetKind::Battery,
            slot_left2: None,
            slot_right2: None,
            net_bits: false,
            disk_drive: None,
            low_at: 20,
            week_sunday: false,
            pill_low: true,
        }
    }
}

impl WidgetConfig {
    /// The filled slots in page order: top left, top right, then the lower two.
    pub fn slots(self) -> impl Iterator<Item = WidgetKind> {
        self.cells().into_iter().flatten()
    }
    /// The four cells in grid order (left, right, lower left, lower right); `None` draws nothing.
    pub fn cells(self) -> [Option<WidgetKind>; 4] {
        [
            Some(self.slot_left),
            Some(self.slot_right),
            self.slot_left2,
            self.slot_right2,
        ]
    }
    /// True when a lower slot is filled: the page is two rows of compact tiles.
    pub fn tall(self) -> bool {
        self.slot_left2.is_some() || self.slot_right2.is_some()
    }
    pub fn uses(self, kind: WidgetKind) -> bool {
        self.slots().any(|k| k == kind)
    }
    /// True when a tile shows a reading that changes by itself (memory, disk, CPU, network).
    pub fn live(self) -> bool {
        self.slots().any(|kind| {
            matches!(
                kind,
                WidgetKind::Memory | WidgetKind::Disk | WidgetKind::Cpu | WidgetKind::Network
            )
        })
    }
}

/// What the notch draws beyond the page's own data: today's water as (drunk, goal), and whether the closed pill
/// carries the low-battery bolt.
#[derive(Clone, Copy, Default)]
pub struct Glance {
    pub water: Option<(u8, u8)>,
    pub low: bool,
}

/// How often the system tiles are read while they show.
pub const STATS_STEP: Duration = Duration::from_secs(1);

/// CPU in use and network speed: differences between two samples `STATS_STEP` apart, taken only while an
/// open notch shows the tiles (`Desktop::refresh_stats`); dropped when it closes.
#[derive(Default)]
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
        assert_eq!(WidgetKind::from_key("memory"), Some(WidgetKind::Memory));
        assert!(
            WidgetKind::ALL
                .iter()
                .all(|k| WidgetKind::from_key(k.key()) == Some(*k))
        );
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
    fn configuration_defaults_and_keys() {
        let config = WidgetConfig::default();
        assert_eq!(config.slot_left, WidgetKind::WeekCalendar);
        assert_eq!(config.slot_right, WidgetKind::Battery);
        for kind in [WidgetKind::WeekCalendar, WidgetKind::Battery] {
            assert_eq!(WidgetKind::from_key(kind.key()), Some(kind));
        }
        assert_eq!(WidgetKind::from_key("unknown"), None);
        assert!(!config.live());
        let stats = WidgetConfig {
            slot_right: WidgetKind::Network,
            ..config
        };
        assert!(stats.live() && !stats.tall());
        // A lower slot makes the page tall and counts for live tiles and for what a page must read.
        let lower = WidgetConfig {
            slot_left2: Some(WidgetKind::Cpu),
            ..config
        };
        assert!(lower.tall() && lower.live() && lower.uses(WidgetKind::Cpu));
        assert_eq!(lower.slots().count(), 3);
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
