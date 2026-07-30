//! Turning a unix epoch second into the `HH:MM` a person reads off their own
//! clock.
//!
//! Hand-rolled because nightcrow has no date crate and a deadline is the only
//! absolute time it ever renders: adding `chrono`/`time` for two integers would
//! buy a dependency and its transitive tree for one format string.

#[cfg(any(not(unix), test))]
const SECS_PER_MINUTE: i64 = 60;
#[cfg(any(not(unix), test))]
const SECS_PER_HOUR: i64 = 3_600;
#[cfg(any(not(unix), test))]
const SECS_PER_DAY: i64 = 86_400;

/// `HH:MM` in the machine's local zone, or `None` when the timestamp is one the
/// platform cannot place.
///
/// `None` rather than a fallback on purpose: a wrong wall-clock time reads as
/// fact, and the caller is expected to show nothing instead.
pub(crate) fn local_hour_minute(epoch: i64) -> Option<String> {
    let (hour, minute) = local_hm(epoch)?;
    Some(format!("{hour:02}:{minute:02}"))
}

// The conversion is identity where `time_t` is 64-bit and a real narrowing check
// where it is 32-bit, so it has to stay even though it looks redundant here.
#[allow(clippy::useless_conversion)]
#[cfg(unix)]
fn local_hm(epoch: i64) -> Option<(u32, u32)> {
    let seconds: libc::time_t = epoch.try_into().ok()?;
    let mut parts: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: `seconds` is a live `time_t` and `parts` a live `tm` for the whole
    // call; `localtime_r` reads the first and writes only into the second, and is
    // the reentrant form precisely so it needs no shared state.
    let filled = unsafe { libc::localtime_r(&seconds, &mut parts) };
    if filled.is_null() {
        return None;
    }
    Some((
        u32::try_from(parts.tm_hour).ok()?,
        u32::try_from(parts.tm_min).ok()?,
    ))
}

/// UTC on platforms with no `localtime_r`. The zone database is the OS's to
/// expose, and guessing an offset would be worse than being explicit about the
/// one this falls back to.
#[cfg(not(unix))]
fn local_hm(epoch: i64) -> Option<(u32, u32)> {
    utc_hm(epoch)
}

/// `HH:MM` in UTC, which is what the epoch already counts.
///
/// `epoch.rem_euclid` rather than `%` so a pre-1970 timestamp lands on the right
/// side of midnight instead of producing a negative hour.
#[cfg(any(not(unix), test))]
fn utc_hm(epoch: i64) -> Option<(u32, u32)> {
    let into_day = epoch.rem_euclid(SECS_PER_DAY);
    Some((
        u32::try_from(into_day / SECS_PER_HOUR).ok()?,
        u32::try_from((into_day % SECS_PER_HOUR) / SECS_PER_MINUTE).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{local_hour_minute, utc_hm};

    #[test]
    fn the_epoch_itself_is_midnight_utc() {
        assert_eq!(utc_hm(0), Some((0, 0)));
    }

    #[test]
    fn a_timestamp_before_the_epoch_stays_inside_the_day() {
        // One minute before 1970 is 23:59, not a negative hour.
        assert_eq!(utc_hm(-60), Some((23, 59)));
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
