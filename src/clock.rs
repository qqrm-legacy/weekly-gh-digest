//! Time and pacing boundary shared by collection and provider calls.
use crate::timestamp::Timestamp;
use std::time::Duration;

pub(super) trait Clock {
    fn now(&self) -> Timestamp;
    fn sleep(&self, duration: Duration);
}

pub(super) struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
