use std::fmt::Write as _;

use crate::daemon::protocol::{DaemonStatus, StatusUnavailable, StatusUnavailableReason};

pub(super) fn render_status(status: &DaemonStatus) -> String {
    let mut output = String::new();
    writeln!(output, "Status: running").unwrap();
    writeln!(output, "PID: {}", status.pid).unwrap();
    writeln!(output, "Version: {}", status.version).unwrap();
    writeln!(
        output,
        "Started at: {}",
        format_started_at(&status.started_at_unix_ms)
    )
    .unwrap();
    writeln!(output, "Uptime: {}", format_uptime(status.uptime_ms)).unwrap();
    writeln!(output, "Endpoint: {}", format_endpoint(&status.endpoint)).unwrap();

    let mut clients = status.attached_clients.clone();
    clients.sort_unstable();
    writeln!(output, "Attached clients: {}", clients.len()).unwrap();
    writeln!(output, "Attached client IDs: {}", format_list(&clients)).unwrap();

    let mut repositories = status.repositories.clone();
    repositories.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.path.cmp(&right.path))
    });
    writeln!(output, "Repositories: {}", repositories.len()).unwrap();
    if repositories.is_empty() {
        writeln!(output, "  (none)").unwrap();
    } else {
        for repository in repositories {
            writeln!(output, "  Repository: {}", display_text(&repository.id)).unwrap();
            writeln!(output, "    Path: {}", display_text(&repository.path)).unwrap();
            writeln!(output, "    Pane count: {}", repository.pane_count).unwrap();
            let mut panes = repository.panes;
            panes.sort_unstable();
            writeln!(output, "    Pane IDs: {}", format_list(&panes)).unwrap();
        }
    }
    output.pop();
    output
}

fn format_list<T: std::fmt::Display>(values: &[T]) -> String {
    if values.is_empty() {
        "(none)".to_string()
    } else {
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn format_started_at(value: &Result<u64, StatusUnavailable>) -> String {
    match value {
        Ok(milliseconds) => format_utc_millis(*milliseconds),
        Err(unavailable) => unavailable_text(&unavailable.reason),
    }
}

fn format_endpoint(value: &Result<String, StatusUnavailable>) -> String {
    match value {
        Ok(endpoint) => display_text(endpoint),
        Err(unavailable) => unavailable_text(&unavailable.reason),
    }
}

fn unavailable_text(reason: &StatusUnavailableReason) -> String {
    let reason = match reason {
        StatusUnavailableReason::ClockBeforeUnixEpoch => "clock before Unix epoch",
        StatusUnavailableReason::EndpointNotUnicode => "endpoint path is not valid Unicode",
    };
    format!("unavailable ({reason})")
}

/// Keep daemon-controlled paths and ids on one terminal line without emitting
/// C0/C1 controls, including escape sequences and OSC payloads.
pub(super) fn display_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                write!(escaped, "\\u{{{:04x}}}", character as u32).unwrap();
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn format_utc_millis(milliseconds: u64) -> String {
    const MILLIS_PER_DAY: u64 = 86_400_000;
    let days = milliseconds / MILLIS_PER_DAY;
    let Some(days) = i64::try_from(days)
        .ok()
        .filter(|days| *days <= i64::MAX - 719_468)
    else {
        return format!("unix-ms:{milliseconds}");
    };
    let day_millis = milliseconds % MILLIS_PER_DAY;
    let hour = day_millis / 3_600_000;
    let minute = (day_millis / 60_000) % 60;
    let second = (day_millis / 1_000) % 60;
    let millis = day_millis % 1_000;
    let (year, month, day) = civil_date(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_date(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_unix_epoch + 719_468;
    let era = (if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    }) / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

fn format_uptime(milliseconds: u64) -> String {
    const MILLIS_PER_SECOND: u64 = 1_000;
    const SECONDS_PER_MINUTE: u64 = 60;
    const MINUTES_PER_HOUR: u64 = 60;
    const HOURS_PER_DAY: u64 = 24;

    let mut seconds = milliseconds / MILLIS_PER_SECOND;
    let days = seconds / (HOURS_PER_DAY * MINUTES_PER_HOUR * SECONDS_PER_MINUTE);
    seconds %= HOURS_PER_DAY * MINUTES_PER_HOUR * SECONDS_PER_MINUTE;
    let hours = seconds / (MINUTES_PER_HOUR * SECONDS_PER_MINUTE);
    seconds %= MINUTES_PER_HOUR * SECONDS_PER_MINUTE;
    let minutes = seconds / SECONDS_PER_MINUTE;
    let seconds = seconds % SECONDS_PER_MINUTE;
    let mut units = Vec::new();
    if days > 0 {
        units.push(format!("{days}d"));
    }
    if hours > 0 {
        units.push(format!("{hours}h"));
    }
    if minutes > 0 {
        units.push(format!("{minutes}m"));
    }
    if seconds > 0 || units.is_empty() {
        units.push(format!("{seconds}s"));
    }
    units.join(" ")
}

#[cfg(test)]
#[path = "status_render_tests.rs"]
mod tests;
