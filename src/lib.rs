//! A token bucket rate limiter for rust which can be used by either a single
//! thread or shared across threads.

#![cfg_attr(feature = "cargo-clippy", deny(warnings))]

#[cfg(feature = "unstable")]
extern crate test;

use std::time::{Duration, Instant};

pub use crate::single_thread::Limiter;

mod single_thread;

/// Maximum acceptable duration in this crate, which is `2^64` nanoseconds,
/// about 20 years.
pub const MAX_DURATION: Duration = Duration::from_nanos(u64::MAX);

/// A builder for a rate limiter.
pub struct Builder {
    capacity: u64,
    quantum: u64,
    start_with: u64,
    interval: Duration,
}

impl Builder {
    /// Creates a new Builder with the default config.
    pub fn new() -> Builder {
        Builder::default()
    }

    /// Build a single thread rate limiter.
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Builder;
    ///
    /// let mut r = Builder::new().single_thread();
    pub fn single_thread(self) -> crate::single_thread::Limiter {
        let Self {
            capacity,
            quantum,
            start_with,
            interval,
        } = self;
        let start_with = start_with.min(capacity);
        crate::single_thread::Limiter {
            capacity,
            quantum,
            available: start_with,
            interval,
            last_tick: Instant::now(),
        }
    }

    /// Sets the number of tokens to add per interval.
    ///
    /// Default value is 1
    pub fn quantum(mut self, quantum: u64) -> Self {
        assert!(quantum > 0);
        self.quantum = quantum;
        self
    }

    /// Sets the number of tokens that the bucket can hold.
    ///
    /// Default value is 1
    pub fn capacity(mut self, capacity: u64) -> Self {
        assert!(capacity > 0);
        self.capacity = capacity;
        self
    }

    /// Set the duration between token adds.
    ///
    /// Default value is `Duration::from_secs(1)`.
    pub fn interval(mut self, interval: Duration) -> Self {
        assert!(interval > Duration::from_secs(0));
        self.interval = interval;
        self
    }

    /// Set the available tokens in the beginning.
    ///
    /// Default value is `capacity`.
    pub fn start_with(mut self, start_with: u64) -> Self {
        self.start_with = start_with;
        self
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            capacity: 1,
            quantum: 1,
            interval: Duration::from_secs(1),
            start_with: 1,
        }
    }
}
