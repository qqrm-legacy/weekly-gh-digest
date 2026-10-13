//! Time and pacing boundary shared by collection and provider calls.
use chrono::{DateTime, Utc};
use std::time::Duration;

pub(super) trait Clock {
    fn now(&self) -> DateTime<Utc>;
    fn sleep(&self, duration: Duration);
}

pub(super) struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
