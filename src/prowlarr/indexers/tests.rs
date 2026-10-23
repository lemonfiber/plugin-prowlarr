use super::{window_start, Duration, SystemTime, DAILY_WINDOW, EPOCH, UNIX_EPOCH};

fn noon() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_786_968_000)
}

#[test]
fn a_window_is_taken_back_from_the_moment_it_is_asked_at() {
    assert_eq!(window_start(noon(), DAILY_WINDOW), "2026-08-16T12:00:00");
}

#[test]
fn a_clock_with_no_room_behind_it_asks_from_the_beginning() {
    assert_eq!(window_start(UNIX_EPOCH, DAILY_WINDOW), EPOCH);
}

#[test]
fn a_clock_beyond_any_calendar_asks_from_the_beginning_too() {
    let absurd = UNIX_EPOCH + Duration::from_secs(3_000_000_000_000);
    assert_eq!(window_start(absurd, DAILY_WINDOW), EPOCH);
}
