//! Configuration for the two expanded-notch widget slots.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    WeekCalendar,
    Battery,
    /// Memory in use.
    Memory,
    /// The system drive's space in use.
    Disk,
}

impl WidgetKind {
    pub const ALL: [Self; 4] = [Self::WeekCalendar, Self::Battery, Self::Memory, Self::Disk];
    pub fn key(self) -> &'static str {
        match self {
            Self::WeekCalendar => "week",
            Self::Battery => "battery",
            Self::Memory => "memory",
            Self::Disk => "disk",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "week" => Some(Self::WeekCalendar),
            "battery" => Some(Self::Battery),
            "memory" => Some(Self::Memory),
            "disk" => Some(Self::Disk),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetConfig {
    pub slot_left: WidgetKind,
    pub slot_right: WidgetKind,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        Self {
            slot_left: WidgetKind::WeekCalendar,
            slot_right: WidgetKind::Battery,
        }
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

/// The system drive's space in use, read when the tile is drawn.
pub fn disk() -> Option<Usage> {
    use windows::{Win32::Storage::FileSystem::GetDiskFreeSpaceExW, core::HSTRING};
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
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
    }
}
