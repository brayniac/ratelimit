//! A token bucket ratelimiter for rust which can be used by either a single
//! thread or shared across threads using a mpsc synchronous channel
//!
//!
//! # Goals
//! * a simple token bucket ratelimiter
//! * usable in single or multi thread code
//!
//! # Future work
//! * consider additional types of ratelimiters
//!
//! # Usage
//!
//! Construct a new `Limiter` and call block between actions. For multi-thread
//! clone the channel sender to pass to the threads, in a separate thread, call
//! run on the `Limiter` in a tight loop.
//!
//! # Examples
//!
//! Construct `Limiter` to do a count-down.
//!
//! ```
//! extern crate ratelimit;
//!
//! use std::time::Duration;
//!
//! let mut ratelimit = ratelimit::Builder::new()
//!     .capacity(1) //number of tokens the bucket will hold
//!     .quantum(1) //add one token per interval
//!     .interval(Duration::new(1, 0)) //add quantum tokens every 1 second
//!     .build();
//!
//! // count down to ignition
//! println!("Count-down....");
//! for i in -10..0 {
//!     println!("T {}", i);
//!     ratelimit.wait();
//! }
//! println!("Ignition!");
//! ```
//!
//! Use the `Limiter` to ratelimit multi-threaded cases
//!
//! ```
//! extern crate ratelimit;
//!
//! use std::thread;
//! use std::time::{Duration, Instant};
//!
//! let mut ratelimit = ratelimit::Builder::new()
//!     .capacity(1) //number of tokens the bucket will hold
//!     .quantum(1) //add one token per interval
//!     .interval(Duration::new(1, 0)) //add quantum tokens every 1 second
//!     .build();
//! let mut handle = ratelimit.make_handle();
//! thread::spawn(move || { ratelimit.run() });
//!
//! // launch threads
//! let mut threads = Vec::new();
//! for _ in 0..10 {
//!     let mut handle = handle.clone();
//!     threads.push(thread::spawn(move || {
//!         handle.wait();
//!         println!("current time: {:?}", Instant::now());
//!     }));
//! }
//! for thread in threads {
//!     thread.join().unwrap();
//! }
//! println!("time's up!");
//! ```

#![cfg_attr(feature = "cargo-clippy", deny(missing_docs))]
#![cfg_attr(feature = "cargo-clippy", deny(warnings))]
#![cfg_attr(feature = "unstable", feature(test))]

#[cfg(feature = "unstable")]
extern crate test;

use std::sync::mpsc;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// A builder for a rate limiter.
pub struct Builder {
    start: Instant,
    capacity: u32,
    quantum: u32,
    interval: Duration,
}

/// Single-threaded rate limiter.
pub struct Limiter {
    config: Builder,
    t0: Instant,
    available: f64,
    tx: Handle,
    rx: mpsc::Receiver<()>,
}

/// Handler for a multi-threaded rate-limiter.
#[derive(Clone)]
pub struct Handle {
    inner: mpsc::SyncSender<()>,
    available: usize,
}
unsafe impl Sync for Handle {}

impl Handle {
    /// Block for 1 token
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Builder;
    /// # use std::thread;
    /// let mut limiter = Builder::default().build();
    /// let mut handle = limiter.make_handle();
    ///
    /// thread::spawn(move || { limiter.run() });
    ///
    /// let mut threads = Vec::new();
    ///
    /// for i in 0..2 {
    ///     let mut handle = handle.clone();
    ///     threads.push(thread::spawn(move || {
    ///         for x in 0..5 {
    ///             handle.wait();
    ///             println!(".");
    ///         }
    ///     }));
    /// }
    pub fn wait(&mut self) {
        if self.available >= 1 {
            self.available -= 1;
        } else {
            loop {
                if self.inner.try_send(()).is_ok() {
                    break;
                }
                sleep(Duration::new(0, 1_000));
            }
        }
    }

    /// Block for `count` tokens
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Builder;
    /// # use std::thread;
    /// let mut limiter = Builder::default().build();
    /// let mut handle = limiter.make_handle();
    ///
    /// thread::spawn(move || { limiter.run() });
    ///
    /// let mut threads = Vec::new();
    ///
    /// for i in 0..2 {
    ///     let mut handle = handle.clone();
    ///     threads.push(thread::spawn(move || {
    ///         for x in 0..5 {
    ///             handle.wait_for(1);
    ///             println!(".");
    ///         }
    ///     }));
    /// }
    pub fn wait_for(&mut self, count: usize) {
        for _ in 0..count {
            self.wait();
        }
    }

    /// Non-blocking wait for one token.
    /// Returns Ok(()) if no wait required.
    /// Returns Err(()) if wait would block.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Builder;
    /// let mut limiter = Builder::new().build();
    /// let mut handle = limiter.make_handle();
    ///
    /// if handle.try_wait().is_ok() {
    ///     println!("token granted");
    /// } else {
    ///     println!("would block");
    /// }
    pub fn try_wait(&mut self) -> Result<(), ()> {
        self.try_wait_for(1)
    }

    /// Non-blocking wait for `count` token.
    /// Returns Ok(()) if no wait required.
    /// Returns Err(()) if wait would block.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Builder;
    /// let mut limiter = Builder::new().build();
    /// let mut handle = limiter.make_handle();
    ///
    /// if handle.try_wait_for(2).is_ok() {
    ///     println!("tokens granted");
    /// } else {
    ///     println!("would block");
    /// }
    pub fn try_wait_for(&mut self, count: usize) -> Result<(), ()> {
        if count <= self.available {
            self.available -= count;
            Ok(())
        } else {
            let needed = count - self.available;
            for _ in 0..needed {
                if self.inner.try_send(()).is_ok() {
                    self.available += 1;
                } else {
                    break;
                }
            }
            if self.available == count {
                self.available = 0;
                Ok(())
            } else {
                Err(())
            }
        }
    }
}

impl Builder {
    /// Creates a new Builder with the default config
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Builder;
    /// let mut b = Builder::new();
    pub fn new() -> Builder {
        Builder::default()
    }


    /// Returns a Limiter from the Builder.
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Builder;
    ///
    /// let mut r = Builder::new().build();
    pub fn build(self) -> Limiter {
        Limiter::configured(self)
    }

    /// Sets the number of tokens to add per interval.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Limiter;
    ///
    /// let mut r = Limiter::configure()
    ///     .quantum(100)
    ///     .build();
    pub fn quantum(mut self, quantum: u32) -> Self {
        self.quantum = quantum;
        self
    }

    /// Sets the number of tokens that the bucket can hold.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Limiter;
    ///
    /// let mut r = Limiter::configure()
    ///     .capacity(100)
    ///     .build();
    pub fn capacity(mut self, capacity: u32) -> Self {
        self.capacity = capacity;
        self
    }

    /// Set the duration between token adds.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Limiter;
    /// # use std::time::Duration;
    ///
    /// let mut r = Limiter::configure()
    ///     .interval(Duration::new(2, 0))
    ///     .build();
    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Alternative to `interval`; sets the number of times
    /// that tokens will be added to the bucket each second.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Limiter;
    ///
    /// let mut rate = 100_000; // 100kHz
    /// let mut r = Limiter::configure()
    ///     .frequency(rate)
    ///     .build();
    pub fn frequency(mut self, cycles: u32) -> Self {
        let mut interval = Duration::new(1, 0);
        interval /= cycles;
        self.interval = interval;
        self
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            capacity: 1,
            quantum: 1,
            interval: Duration::new(1, 0),
        }
    }
}

impl Default for Limiter {
    fn default() -> Limiter {
        Builder::default().build()
    }
}

impl Limiter {
    /// create a new default ratelimit instance
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Limiter;
    ///
    /// let mut r = Limiter::new();
    pub fn new() -> Limiter {
        Default::default()
    }

    /// create a new `Config` for building a custom `Limiter`
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Limiter;
    ///
    /// let mut r = Limiter::configure().build();
    pub fn configure() -> Builder {
        Builder::default()
    }

    // internal function for building a Limiter from its Config
    fn configured(config: Builder) -> Limiter {
        let (tx, rx) = mpsc::sync_channel(config.capacity as usize);
        for _ in 0..config.capacity {
            let _ = tx.send(());
        }
        let t0 = config.start;
        Limiter {
            config: config,
            t0: t0,
            available: 0.0,
            tx: Handle {
                inner: tx,
                available: 0,
            },
            rx: rx,
        }
    }

    /// Run the limiter, intended for multi-threaded use
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Limiter;
    /// # use std::thread;
    ///
    /// let mut limiter = Limiter::new();
    ///
    /// let mut handle = limiter.make_handle();
    ///
    /// thread::spawn(move || { limiter.run(); });
    ///
    /// handle.wait();
    pub fn run(&mut self) {
        // pre-fill the queue
        loop {
            if self.tx.inner.try_send(()).is_err() {
                break;
            }
        }
        // set available to 0
        self.available = 0.0;
        // reset t0
        self.t0 = Instant::now();
        loop {
            self.run_once();
        }
    }

    /// Run the limiter for a single tick
    fn run_once(&mut self) {
        let quantum = self.config.quantum as usize;
        self.wait_for(quantum);
        let mut unused = 0;
        for _ in 0..self.config.quantum {
            match self.rx.try_recv() {
                Ok(..) => {}
                Err(_) => {
                    unused += 1;
                }
            }
        }
        self.give(f64::from(unused));
    }

    /// Create a Handle for multi-thread use
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Limiter;
    ///
    /// let mut limiter = Limiter::new();
    ///
    /// let mut handle = limiter.make_handle();
    ///
    /// match handle.try_wait() {
    ///     Ok(_) => {
    ///         println!("not limited");
    ///     },
    ///     Err(_) => {
    ///         println!("was limited");
    ///     },
    /// }
    pub fn make_handle(&mut self) -> Handle {
        self.tx.clone()
    }

    /// Blocking wait for 1 token.
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Limiter;
    ///
    /// let mut limiter = Limiter::new();
    /// for i in -10..0 {
    ///     println!("T {}...", i);
    ///     limiter.wait();
    /// }
    /// println!("Ignition!");
    pub fn wait(&mut self) {
        self.wait_for(1)
    }

    /// Blocking wait for `count` tokens.
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Limiter;
    ///
    /// let mut limiter = Limiter::new();
    /// for i in 1..3 {
    ///     println!("{}...", i);
    ///     limiter.wait_for(i);
    /// }
    pub fn wait_for(&mut self, count: usize) {
        if let Some(wait) = self.take(Instant::now(), count) {
            sleep(wait);
        }
    }

    // Internal function to add tokens to the bucket
    fn give(&mut self, count: f64) {
        self.available += count;

        let capacity = f64::from(self.config.capacity);

        if self.available >= capacity {
            self.available = capacity;
        }
    }

    // Return time to sleep until `count` tokens are available
    fn take(&mut self, t1: Instant, count: usize) -> Option<Duration> {

        // we don't need any tokens, just return
        if count == 0 {
            return None;
        }

        // we have all tokens we need, fast path
        if self.available >= count as f64 {
            self.available -= count as f64;
            return None;
        }

        // do the accounting
        self.tick(t1);

        // do we have the tokens now?
        if self.available >= count as f64 {
            self.available -= count as f64;
            return None;
        }

        // we must wait
        let need = count as f64 - self.available;
        let wait = self.config.interval * need.ceil() as u32;
        self.available -= count as f64;

        Some(wait)
    }

    // move the time forward and do bookkeeping
    fn tick(&mut self, t1: Instant) {
        let t0 = self.t0;
        let cycles = cycles(t1.duration_since(t0), self.config.interval);
        let tokens = cycles * f64::from(self.config.quantum);

        self.give(tokens);
        self.t0 = t1;
    }
}

// returns the number of cycles of period length within the duration
fn cycles(duration: Duration, period: Duration) -> f64 {
    let d = 1_000_000_000 * duration.as_secs() as u64 + u64::from(duration.subsec_nanos());
    let p = 1_000_000_000 * period.as_secs() as u64 + u64::from(period.subsec_nanos());
    d as f64 / p as f64
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "unstable")]
    extern crate test;

    #[cfg(feature = "unstable")]
    use std::thread;

    use super::*;
    use std::time::Duration;

    #[test]
    fn test_cycles() {
        let d = Duration::new(10, 0);
        assert_eq!(cycles(d, Duration::new(1, 0)), 10.0);

        let d = Duration::new(1, 500_000_000);
        assert_eq!(cycles(d, Duration::new(1, 0)), 1.5);

        let d = Duration::new(1, 0);
        assert_eq!(cycles(d, Duration::new(0, 1000000)), 1000.0);

        let d = Duration::new(1, 0);
        assert_eq!(cycles(d, Duration::new(0, 2000000)), 500.0);

        let d = Duration::new(0, 1000);
        assert_eq!(cycles(d, Duration::new(0, 2000000)), 0.0005);

        let d = Duration::new(0, 0);
        assert_eq!(cycles(d, Duration::new(0, 2000000)), 0.0);

        let d = Duration::new(0, 1);
        assert_eq!(cycles(d, Duration::new(0, 1)), 1.0);
    }

    #[test]
    fn test_give() {
        let mut r = Builder::default().capacity(1000).build();

        assert_eq!(r.available, 0.0);

        r.give(1.0);
        assert_eq!(r.available, 1.0);

        r.give(1.5);
        assert_eq!(r.available, 2.5);
    }

    #[test]
    fn test_tick() {
        let mut r = Builder::default().capacity(1000).build();
        let t = r.t0;
        r.tick(t + Duration::new(1, 0));
        assert_eq!(r.available, 1.0);
        let t = r.t0;
        r.tick(t + Duration::new(0, 500_000_000));
        assert_eq!(r.available, 1.5);
    }

    #[test]
    fn test_take() {
        let mut r = Builder::default().frequency(1000).capacity(1000).build();
        let t = r.t0;
        assert_eq!(r.take(t + Duration::new(1, 0), 1000), None);
        assert_eq!(
            r.take(t + Duration::new(1, 0), 1000),
            Some(Duration::new(1, 0))
        );
        assert_eq!(
            r.take(t + Duration::new(2, 0), 1000),
            Some(Duration::new(1, 0))
        );
        assert_eq!(r.take(t + Duration::new(4, 0), 1000), None);
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn interval_1000ns(b: &mut test::Bencher) {
        let mut r = Builder::default()
            .capacity(10000)
            .interval(Duration::new(0, 1_000))
            .build();
        b.iter(|| r.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn interval_500ns(b: &mut test::Bencher) {
        let mut r = Builder::default()
            .capacity(10000)
            .interval(Duration::new(0, 500))
            .build();
        b.iter(|| r.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn interval_100ns(b: &mut test::Bencher) {
        let mut r = Builder::default()
            .capacity(10000)
            .interval(Duration::new(0, 100))
            .build();
        b.iter(|| r.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn interval_50ns(b: &mut test::Bencher) {
        let mut r = Builder::default()
            .capacity(10000)
            .interval(Duration::new(0, 50))
            .build();
        b.iter(|| r.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn multi_interval_1000ns(b: &mut test::Bencher) {
        let mut r = Builder::default()
            .capacity(10000)
            .interval(Duration::new(0, 1_000))
            .build();
        let mut h = r.make_handle();
        thread::spawn(move || { r.run(); });
        b.iter(|| h.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn multi_interval_500ns(b: &mut test::Bencher) {
        let mut r = Builder::default()
            .capacity(10000)
            .interval(Duration::new(0, 500))
            .build();
        let mut h = r.make_handle();
        thread::spawn(move || { r.run(); });
        b.iter(|| h.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn multi_interval_100ns(b: &mut test::Bencher) {
        let mut r = Builder::default()
            .capacity(10000)
            .interval(Duration::new(0, 100))
            .build();
        let mut h = r.make_handle();
        thread::spawn(move || { r.run(); });
        b.iter(|| h.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn multi_interval_50ns(b: &mut test::Bencher) {
        let mut r = Builder::default()
            .capacity(10000)
            .interval(Duration::new(0, 50))
            .build();
        let mut h = r.make_handle();
        thread::spawn(move || { r.run(); });
        b.iter(|| h.wait());
    }
}
