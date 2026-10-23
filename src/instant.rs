use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lemonfiber_manifest::Date;

const DAY: u64 = 86_400;

const HOUR: u64 = 3_600;

const MINUTE: u64 = 60;

const EPOCH: Date = Date {
    year: 1970,
    month: 1,
    day: 1,
};

pub(crate) fn written(at: SystemTime) -> Option<String> {
    let seconds = at.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let date = Date::from_unix_seconds(i64::try_from(seconds).unwrap_or(i64::MAX))?;
    let clock = seconds % DAY;
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        date.year,
        date.month,
        date.day,
        clock / HOUR,
        (clock % HOUR) / MINUTE,
        clock % MINUTE
    ))
}

pub(crate) fn read(stamp: &str) -> Option<SystemTime> {
    let (day, rest) = stamp.split_once('T')?;
    let date = Date::parse(day)?;
    let rest = rest.strip_suffix('Z').unwrap_or(rest);
    let (clock, fraction) = rest.split_once('.').unwrap_or((rest, ""));
    if !fraction.chars().all(|digit| digit.is_ascii_digit()) {
        return None;
    }
    let mut fields = clock.split(':');
    let hours = field(fields.next(), 23)?;
    let minutes = field(fields.next(), 59)?;
    let seconds = field(fields.next(), 59)?;
    if fields.next().is_some() {
        return None;
    }
    let days = u64::try_from(EPOCH.days_until(date)).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(days * DAY + hours * HOUR + minutes * MINUTE + seconds))
}

fn field(text: Option<&str>, most: u64) -> Option<u64> {
    let value: u64 = text?.parse().ok()?;
    (value <= most).then_some(value)
}

#[cfg(test)]
mod tests;
