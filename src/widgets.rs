//! Configuration for the two expanded-notch widget slots.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    WeekCalendar,
    Battery,
}

impl WidgetKind {
    pub const ALL: [Self; 2] = [Self::WeekCalendar, Self::Battery];
    pub fn key(self) -> &'static str {
        match self {
            Self::WeekCalendar => "week",
            Self::Battery => "battery",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "week" => Some(Self::WeekCalendar),
            "battery" => Some(Self::Battery),
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
