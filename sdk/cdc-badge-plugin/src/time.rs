//! \file
//! \brief Time and RTC access.

use crate::ffi::{self, HostTm};

/// \brief Monotonic uptime in milliseconds since boot.
/// \return Milliseconds elapsed since the badge powered on.
pub fn uptime_ms() -> u64 {
    unsafe { ffi::host_uptime_ms() }
}

/// \brief Current wall-clock time as a Unix timestamp.
/// \return Seconds since the Unix epoch, or `0` when the RTC is not set.
pub fn unix_time() -> i64 {
    unsafe { ffi::host_unix_time() }
}

/// \brief Whether the system clock has been initialised.
/// \return `true` once the RTC has been synchronised.
pub fn is_time_set() -> bool {
    unsafe { ffi::host_is_time_set() }
}

/// \brief Configured timezone offset from UTC.
/// \return Offset in seconds (east of UTC positive).
pub fn timezone_offset() -> i32 {
    unsafe { ffi::host_timezone_offset() }
}

/// \brief Decomposed local time fields.
#[derive(Debug, Clone, Copy)]
pub struct LocalTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub weekday: u8,
}

/// \brief Read the current local time.
/// \return The decomposed time, or `None` if the RTC has no value yet.
pub fn local_time() -> Option<LocalTime> {
    let mut tm = HostTm {
        year: 0,
        month: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
        weekday: 0,
    };
    let rc = unsafe { ffi::host_local_time(&mut tm) };
    if rc == 0 {
        Some(LocalTime {
            year: tm.year,
            month: tm.month,
            day: tm.day,
            hour: tm.hour,
            minute: tm.minute,
            second: tm.second,
            weekday: tm.weekday,
        })
    } else {
        None
    }
}
