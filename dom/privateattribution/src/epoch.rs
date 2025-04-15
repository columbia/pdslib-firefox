use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

// all timestamps are in milliseconds, to correspond with
// JS's Date.now()

// note: Date.now() might have anti-fingerprinting that
// rounds to the nearest 2ms. Should we have it too?

pub const DAY_IN_MILLI: u64 = 1000 * 60 * 60 * 24;
pub const EPOCH_DURATION: Duration = Duration::from_millis(7 * DAY_IN_MILLI);

pub fn timestamp_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

pub fn timestamp_to_epoch(timestamp: u64) -> usize {
    (timestamp / EPOCH_DURATION.as_millis() as u64) as usize
}

pub fn epoch_now() -> usize {
    timestamp_to_epoch(timestamp_now())
}

pub fn days_ago_to_epoch(days_ago: usize) -> usize {
    // note: should Date::now() be passed in as an argument,
    // to ensure the same time is used for all calculations?
    // (that's how FF did it)
    let now = timestamp_now();

    let days_ago = days_ago as u64;
    let days_ago_milli = days_ago * DAY_IN_MILLI;
    let target_time = now - days_ago_milli;

    timestamp_to_epoch(target_time)
}
