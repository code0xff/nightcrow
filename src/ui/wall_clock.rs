//! Turning a unix epoch second into the `HH:MM` a person reads off their own
//! clock, hand-rolled because adding a date crate for two format strings
//! would buy a dependency and its transitive tree.

#[cfg(any(not(any(unix, windows)), test))]
const SECS_PER_MINUTE: i64 = 60;
#[cfg(any(not(any(unix, windows)), test))]
const SECS_PER_HOUR: i64 = 3_600;
#[cfg(any(not(any(unix, windows)), test))]
const SECS_PER_DAY: i64 = 86_400;

/// `HH:MM` in the machine's local zone, or `None` when the timestamp is one
/// the platform cannot place. `None` rather than a fallback on purpose: a
/// wrong wall-clock time reads as fact, and the caller is expected to show
/// nothing instead.
pub(crate) fn local_hour_minute(epoch: i64) -> Option<String> {
    let t = local_parts(epoch)?;
    Some(format!("{:02}:{:02}", t.hour, t.minute))
}

/// `YYYY-MM-DD HH:MM` in the machine's local zone, with the same `None`
/// contract as [`local_hour_minute`].
pub(crate) fn local_date_time(epoch: i64) -> Option<String> {
    let t = local_parts(epoch)?;
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        t.year, t.month, t.day, t.hour, t.minute
    ))
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DateTimeParts {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

// The conversion is identity where `time_t` is 64-bit and a real narrowing check
// where it is 32-bit, so it has to stay even though it looks redundant here.
#[allow(clippy::useless_conversion)]
#[cfg(unix)]
fn local_parts(epoch: i64) -> Option<DateTimeParts> {
    let seconds: libc::time_t = epoch.try_into().ok()?;
    let mut parts: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: `seconds` is a live `time_t` and `parts` a live `tm` for the
    // whole call; `localtime_r` reads the first and writes only into the second.
    let filled = unsafe { libc::localtime_r(&seconds, &mut parts) };
    if filled.is_null() {
        return None;
    }
    Some(DateTimeParts {
        // `tm` counts years from 1900 and months from 0.
        year: i64::from(parts.tm_year) + 1900,
        month: u32::try_from(parts.tm_mon).ok()? + 1,
        day: u32::try_from(parts.tm_mday).ok()?,
        hour: u32::try_from(parts.tm_hour).ok()?,
        minute: u32::try_from(parts.tm_min).ok()?,
    })
}

/// Windows: convert the epoch through `FileTimeToLocalFileTime` so the
/// machine's current time-zone rules apply. Pre-1970 timestamps cannot be
/// represented as an unsigned `FILETIME` and return `None` — the caller
/// already handles that.
#[cfg(windows)]
fn local_parts(epoch: i64) -> Option<DateTimeParts> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    use windows_sys::Win32::System::Time::FileTimeToSystemTime;

    // Windows FILETIME counts 100-nanosecond intervals since 1601-01-01 UTC;
    // Unix epoch is 1970-01-01.
    const EPOCH_OFFSET_SECS: u64 = 11_644_473_600;
    const HNS_PER_SEC: u64 = 10_000_000;

    let epoch_u64 = u64::try_from(epoch).ok()?;
    let hns = epoch_u64
        .checked_add(EPOCH_OFFSET_SECS)?
        .checked_mul(HNS_PER_SEC)?;

    let ft = FILETIME {
        dwLowDateTime: (hns & 0xFFFF_FFFF) as u32,
        dwHighDateTime: (hns >> 32) as u32,
    };

    let mut local_ft = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };

    // SAFETY: local_ft and st are live stack variables; the functions write
    // only into them.
    let ok = unsafe {
        FileTimeToLocalFileTime(&ft, &mut local_ft) != 0
            && FileTimeToSystemTime(&local_ft, &mut st) != 0
    };

    if !ok {
        return None;
    }

    Some(DateTimeParts {
        year: i64::from(st.wYear),
        month: u32::from(st.wMonth),
        day: u32::from(st.wDay),
        hour: u32::from(st.wHour),
        minute: u32::from(st.wMinute),
    })
}

/// UTC on platforms with no `localtime_r`: guessing an offset would be worse
/// than being explicit about the one this falls back to.
#[cfg(not(any(unix, windows)))]
fn local_parts(epoch: i64) -> Option<DateTimeParts> {
    utc_parts(epoch)
}

/// `HH:MM` in UTC, which is what the epoch already counts. `rem_euclid`
/// rather than `%` so a pre-1970 timestamp lands on the right side of midnight
/// instead of producing a negative hour.
#[cfg(any(not(any(unix, windows)), test))]
fn utc_parts(epoch: i64) -> Option<DateTimeParts> {
    let into_day = epoch.rem_euclid(SECS_PER_DAY);
    let (year, month, day) = civil_from_days(epoch.div_euclid(SECS_PER_DAY))?;
    Some(DateTimeParts {
        year,
        month,
        day,
        hour: u32::try_from(into_day / SECS_PER_HOUR).ok()?,
        minute: u32::try_from((into_day % SECS_PER_HOUR) / SECS_PER_MINUTE).ok()?,
    })
}

/// Days since the epoch to a proleptic Gregorian date, via Howard Hinnant's
/// `civil_from_days`: shift the era to start in March so the leap day lands at
/// the end of a year and the month arithmetic needs no table.
#[cfg(any(not(any(unix, windows)), test))]
fn civil_from_days(days: i64) -> Option<(i64, u32, u32)> {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    Some((
        if m <= 2 { y + 1 } else { y },
        u32::try_from(m).ok()?,
        u32::try_from(d).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{civil_from_days, local_date_time, local_hour_minute, utc_parts};

    fn hm(epoch: i64) -> Option<(u32, u32)> {
        utc_parts(epoch).map(|t| (t.hour, t.minute))
    }

    #[test]
    fn the_epoch_itself_is_midnight_utc() {
        assert_eq!(hm(0), Some((0, 0)));
    }

    #[test]
    fn a_timestamp_before_the_epoch_stays_inside_the_day() {
        // One minute before 1970 is 23:59, not a negative hour.
        assert_eq!(hm(-60), Some((23, 59)));
    }

    #[test]
    fn the_epoch_itself_is_the_first_of_january_1970() {
        assert_eq!(civil_from_days(0), Some((1970, 1, 1)));
    }

    #[test]
    fn a_day_before_the_epoch_is_the_last_of_december_1969() {
        assert_eq!(civil_from_days(-1), Some((1969, 12, 31)));
    }

    #[test]
    fn a_leap_day_is_placed_on_the_twenty_ninth_of_february() {
        // 2024-02-29 00:00 UTC.
        assert_eq!(civil_from_days(19_782), Some((2024, 2, 29)));
    }

    #[test]
    fn a_commit_time_renders_as_a_date_and_a_clock() {
        let rendered = local_date_time(1_700_000_000).expect("a plausible commit time must render");
        assert_eq!(rendered.len(), 16, "{rendered}");
    }

    #[test]
    fn a_commit_time_no_platform_clock_can_place_renders_nothing() {
        assert_eq!(local_date_time(i64::MIN), None);
    }

    #[test]
    fn a_deadline_renders_as_two_padded_fields() {
        let rendered = local_hour_minute(1_700_000_000).expect("a plausible deadline must render");
        assert_eq!(rendered.len(), 5, "{rendered}");
        let (hour, minute) = rendered.split_once(':').expect("HH:MM");
        assert!(hour.parse::<u32>().unwrap() < 24, "{rendered}");
        assert!(minute.parse::<u32>().unwrap() < 60, "{rendered}");
    }

    #[test]
    fn a_timestamp_no_platform_clock_can_place_renders_nothing() {
        assert_eq!(local_hour_minute(i64::MIN), None);
    }
}
