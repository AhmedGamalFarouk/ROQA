use windows::{
    Win32::{
        Globalization::{DATE_LONGDATE, GetDateFormatEx, GetTimeFormatEx, TIME_NOSECONDS},
        System::SystemInformation::GetLocalTime,
    },
    core::PCWSTR,
};

pub struct Clock {
    /// User-locale time without seconds (12/24-hour follows Windows settings).
    pub time: String,
    pub date: String,
    /// Wait this long to wake exactly once per minute; nothing else redraws the clock.
    pub next_minute_ms: u32,
}

pub fn now() -> Clock {
    // SAFETY: plain value query.
    let local = unsafe { GetLocalTime() };
    let mut time = [0u16; 64];
    let mut date = [0u16; 128];
    // SAFETY: NULL locale = user default; output buffers are bounded slices and the lengths include the NUL.
    let (time_len, date_len) = unsafe {
        (
            GetTimeFormatEx(
                PCWSTR::null(),
                TIME_NOSECONDS,
                Some(&local),
                PCWSTR::null(),
                Some(&mut time),
            ),
            GetDateFormatEx(
                PCWSTR::null(),
                DATE_LONGDATE,
                Some(&local),
                PCWSTR::null(),
                Some(&mut date),
                PCWSTR::null(),
            ),
        )
    };
    let text = |buffer: &[u16], length: i32| {
        String::from_utf16_lossy(&buffer[..(length.max(1) as usize - 1).min(buffer.len())])
    };
    Clock {
        time: text(&time, time_len),
        date: text(&date, date_len),
        next_minute_ms: next_minute_ms(local.wSecond, local.wMilliseconds),
    }
}

fn next_minute_ms(second: u16, millisecond: u16) -> u32 {
    // +20 ms lands just after the boundary so the new minute is already visible.
    (60_000u32).saturating_sub(u32::from(second.min(59)) * 1000 + u32::from(millisecond.min(999)))
        + 20
}

#[cfg(test)]
mod tests {
    #[test]
    fn wakes_just_after_each_minute_boundary() {
        assert_eq!(super::next_minute_ms(0, 0), 60_020);
        assert_eq!(super::next_minute_ms(59, 999), 21);
        assert_eq!(super::next_minute_ms(30, 500), 29_520);
        assert_eq!(super::next_minute_ms(60, 0), 1_020); // leap second clamps, never underflows
    }
}
