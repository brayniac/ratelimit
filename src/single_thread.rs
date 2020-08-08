use std::time::{Duration, Instant};

use crate::{Builder, MAX_DURATION};

impl Default for Limiter {
    fn default() -> Limiter {
        crate::Builder::default().single_thread()
    }
}

/// Single-threaded rate limiter.
pub struct Limiter {
    pub(crate) capacity: u64,
    pub(crate) quantum: u64,
    pub(crate) interval: Duration,
    pub(crate) available: u64,
    pub(crate) last_tick: Instant,
}

impl Limiter {
    pub fn builder() -> Builder {
        Builder::default()
    }

    fn update(&mut self) {
        let elapsed = self.last_tick.elapsed();
        let round = elapsed.as_nanos() / self.interval.as_nanos();
        let align_elapsed = round * self.interval.as_nanos();

        if align_elapsed > u64::max_value() as u128 {
            unimplemented!("since last update, it has been at least 20 years, I don't think this could be real");
        }

        let round = round as u64;
        let align_elapsed = align_elapsed as u64;

        self.available = self
            .quantum
            .saturating_mul(round)
            .saturating_add(self.available)
            .min(self.capacity);
        self.last_tick += Duration::from_nanos(align_elapsed);
    }

    pub fn wait_for(&mut self, count: u64) {
        match self.virtual_wait_for(count) {
            Ok(new_available) => {
                self.available = new_available;
            }
            Err(would_wait) => {
                std::thread::sleep(would_wait);
                self.wait_for(count);
            }
        }
    }

    pub fn try_wait_for(&mut self, count: u64) -> Result<(), ()> {
        match self.virtual_wait_for(count) {
            Ok(new_available) => {
                self.available = new_available;
                Ok(())
            }
            Err(_) => Err(()),
        }
    }

    /// No action to actually wait.
    ///
    /// If token is enough, returns `Ok(new_available)`.
    /// Otherwise, returns `Err(would_wait)`
    ///
    /// [MAX_DURATION]: crate::MAX_DURATION
    ///
    /// If `would_wait` is too large, [MAX_DURATION] would be returned.
    pub fn virtual_wait_for(&mut self, count: u64) -> Result<u64, Duration> {
        let now = Instant::now();

        self.update();
        if let Some(available) = self.available.checked_sub(count) {
            return Ok(available);
        }

        let required = count - self.available;
        let mut required_round = required / self.quantum;
        if required % self.quantum != 0 {
            required_round += 1;
        }
        let required_nanos = self
            .interval
            .as_nanos()
            .saturating_mul(required_round as u128);
        if required_nanos >= MAX_DURATION.as_nanos() {
            return Err(MAX_DURATION);
        }

        let required = Duration::from_nanos(required_round as u64);
        let until = self.last_tick + required;
        Err(until - now)
    }
}
