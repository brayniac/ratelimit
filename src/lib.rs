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
//! use std::thread;
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
//!
//! // make a multithreaded version
//! let multi = ratelimit.sync();
//!
//! // count down in a more awkward way
//! let mut threads = Vec::new();
//! static NUMS: &[&str] = &["Three", "Two", "One"];
//! for num in &NUMS[..] {
//!     let handle = multi.make_handle();
//!     threads.push(thread::spawn(move || {
//!         handle.wait();
//!         println!("{}!", num);
//!     }));
//! }
//! for thread in threads {
//!     thread.join().unwrap();
//! }
//! println!("Ignition?");
//! ```
//!
//! Construct a static `SyncLimiter` to rate-limit an entire library.
//!
//! ```
//! #[macro_use]
//! extern crate lazy_static;
//! extern crate ratelimit;
//!
//! use std::thread;
//! use std::time::Duration;
//!
//! // our rate limiter (kept private to the outside world)
//! lazy_static! {
//!     static ref RATE_LIMITER: ratelimit::SyncLimiter = ratelimit::Builder::new()
//!         .capacity(1)
//!         .quantum(1)
//!         .interval(Duration::new(1, 0))
//!         .build_sync();
//! }
//!
//! fn do_some_work() {
//!     let handle = RATE_LIMITER.make_handle();
//!     thread::spawn(move || {
//!         handle.wait();
//!         // do our work
//!     });
//! }
//!
//! # fn main() {
//! // do a bunch of work
//! for i in -10..0 {
//!     do_some_work();
//! }
//! # }
//! ```

#![cfg_attr(test, deny(warnings))]

#![cfg_attr(feature = "unstable", feature(test))]

#[cfg(feature = "unstable")]
extern crate test;

use std::sync::{Arc, atomic, mpsc};
use std::thread::{sleep, spawn};
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
    tx: mpsc::SyncSender<()>,
    rx: mpsc::Receiver<()>,
}

/// Handler for a multi-threaded rate-limiter.
#[derive(Clone)]
pub struct Handle {
    inner: mpsc::SyncSender<()>,
}
unsafe impl Sync for Handle {}
impl Handle {
    /// Waits for a single token.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::SyncLimiter;
    /// # use std::thread;
    /// let multi = SyncLimiter::default();
    ///
    /// for i in 0..2 {
    ///     let s = multi.make_handle();
    ///     thread::spawn(move || {
    ///         for x in 0..5 {
    ///             s.wait();
    ///             println!(".");
    ///         }
    ///     });
    /// }
    pub fn wait(&self) {
        // NOTE: we *don't* unwrap the value here because the only case where this would
        // actually happen is if the `SyncLimiter` was dropped, thus dropping the original
        // `Limiter`. this behaviour is documented in the docs for `SyncLimiter`
        let _ = self.inner.send(());
    }

    /// Waits for `count` tokens.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::SyncLimiter;
    /// # use std::thread;
    /// let multi = SyncLimiter::default();
    ///
    /// for i in 0..2 {
    ///     let s = multi.make_handle();
    ///     thread::spawn(move || {
    ///         for x in 0..5 {
    ///             s.wait_for(1);
    ///             println!(".");
    ///         }
    ///     });
    /// }
    pub fn wait_for(&self, count: u32) {
        for _ in 0..count {
            self.wait();
        }
    }

    /// Checks if we would be able to get a token, and returns whether we got it.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::SyncLimiter;
    /// let multi = SyncLimiter::default();
    /// let handle = multi.make_handle();
    ///
    /// if handle.try_get() {
    ///     println!("not limited");
    /// } else {
    ///     println!("was limited");
    /// }
    #[cfg_attr(feature = "cargo-clippy", allow(match_same_arms))]
    pub fn try_get(&self) -> bool {
        match self.inner.try_send(()) {
            // if it sent, yes, we sent
            Ok(()) => true,

            // this also counts as being sent, because `wait` will still continue on err
            Err(mpsc::TrySendError::Disconnected(())) => true,

            // this means that we would have to wait
            Err(mpsc::TrySendError::Full(())) => false,
        }
    }
}

impl Builder {
    /// Creates a new builder.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Builder;
    /// let mut b = Builder::new();
    pub fn new() -> Builder {
        Builder::default()
    }

    /// Builds a rate limiter from the given config.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Builder;
    /// let mut r = Builder::new().build();
    pub fn build(self) -> Limiter {
        Limiter::new(self)
    }

    /// Builds a multi-threaded rate limiter from the given config.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Builder;
    /// let mut r = Builder::new().build_sync();
    pub fn build_sync(self) -> SyncLimiter {
        Limiter::new(self).sync()
    }

    /// Sets the number of tokens that are added at each interval.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Builder;
    /// let mut r = Builder::new()
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
    /// # use ratelimit::Builder;
    /// let mut r = Builder::new()
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
    /// # use ratelimit::Builder;
    /// # use std::time::Duration;
    /// let mut r = Builder::new()
    ///     .interval(Duration::new(2, 0))
    ///     .build();
    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Alternative to `interval`; sets the number of tokens adds per second.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Builder;
    /// let mut rate = 100_000; // 100kHz
    /// let mut r = Builder::new()
    ///     .frequency(rate)
    ///     .build();
    pub fn frequency(mut self, per_sec: u32) -> Self {
        let mut interval = Duration::new(1, 0);
        interval /= per_sec;
        self.interval = interval;
        self
    }
}

impl Default for Builder {
    fn default() -> Builder {
        Builder {
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

/// Multi-threaded rate limiter.
///
/// If the rate limiter is dropped before any handles, then the handles will always succeed without
/// any rate limiting. Similarly, if this limiter is leaked, all handles will continue to rate limit
/// properly, although the thread handling the requests will continue to run until the program
/// exits.
pub struct SyncLimiter {
    run_thread: Arc<atomic::AtomicBool>,
    handle: Handle,
}
impl SyncLimiter {
    /// Gets a handle to this rate limiter.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::SyncLimiter;
    /// # use std::thread;
    ///
    /// let multi = SyncLimiter::default();
    ///
    /// for i in 0..2 {
    ///     let s = multi.make_handle();
    ///     thread::spawn(move || {
    ///         for x in 0..5 {
    ///             s.wait();
    ///             println!(".");
    ///         }
    ///     });
    /// }
    pub fn make_handle(&self) -> Handle {
        self.handle.clone()
    }
}
impl From<Limiter> for SyncLimiter {
    fn from(mut rl: Limiter) -> Self {
        let atomic = Arc::new(atomic::AtomicBool::new(false));
        let cond = atomic.clone();

        let handle = Handle { inner: rl.tx.clone() };
        spawn(move || while cond.load(atomic::Ordering::Relaxed) {
            rl.run();
        });

        SyncLimiter {
            run_thread: atomic,
            handle: handle,
        }
    }
}
impl Default for SyncLimiter {
    fn default() -> Self {
        Limiter::default().into()
    }
}
impl Drop for SyncLimiter {
    fn drop(&mut self) {
        self.run_thread.store(false, atomic::Ordering::Relaxed)
    }
}

impl Limiter {
    fn new(config: Builder) -> Limiter {
        let (tx, rx) = mpsc::sync_channel(config.capacity as usize);
        let t0 = config.start;
        Limiter {
            config: config,
            t0: t0,
            available: 0.0,
            tx: tx,
            rx: rx,
        }
    }

    // runs the rate limiter; for multithreaded version only
    fn run(&mut self) {
        let quantum = self.config.quantum;
        self.wait_for(quantum);
        let mut unused = 0;
        for _ in 0..self.config.quantum {
            match self.rx.try_recv() {
                Ok(()) => {}
                Err(_) => {
                    unused += 1;
                }
            }
        }
        self.give(unused as f64);
    }

    /// Promotes this rate-limiter to a multithreaded version.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Limiter;
    /// let mut s = Limiter::default().sync();
    pub fn sync(self) -> SyncLimiter {
        SyncLimiter::from(self)
    }

    /// Waits for a single token.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Limiter;
    /// let mut r = Limiter::default();
    /// for i in -10..0 {
    ///     println!("T {}...", i);
    ///     r.wait();
    /// }
    /// println!("Ignition!");
    pub fn wait(&mut self) {
        self.wait_for(1)
    }

    /// Waits for `count` tokens.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ratelimit::Limiter;
    /// let mut r = Limiter::default();
    /// for i in -10..0 {
    ///     println!("T {}...", i);
    ///     r.wait_for(1);
    /// }
    /// println!("Ignition!");
    pub fn wait_for(&mut self, count: u32) {
        if let Some(wait) = self.take(Instant::now(), count) {
            sleep(wait);
        }
    }

    // internal function to add tokens
    fn give(&mut self, count: f64) {
        self.available += count;

        if self.available >= self.config.capacity as f64 {
            self.available = self.config.capacity as f64;
        }
    }

    // return time to sleep until tokens are available
    fn take(&mut self, t1: Instant, tokens: u32) -> Option<Duration> {

        // we don't need any tokens, just return
        if tokens == 0 {
            return None;
        }

        // we have all tokens we need, fast path
        if self.available >= tokens as f64 {
            self.available -= tokens as f64;
            return None;
        }

        // do the accounting
        self.tick(t1);

        // do we have the tokens now?
        if self.available >= tokens as f64 {
            self.available -= tokens as f64;
            return None;
        }

        // we must wait
        let need = tokens as f64 - self.available;
        let wait = self.config.interval * need.ceil() as u32;
        self.available -= tokens as f64;

        Some(wait)
    }

    // move the time forward and do bookkeeping
    fn tick(&mut self, t1: Instant) {
        let t0 = self.t0;
        let cycles = cycles(t1.duration_since(t0), self.config.interval);
        let tokens = cycles * self.config.quantum as f64;

        self.give(tokens);
        self.t0 = t1;
    }
}

// returns the number of cycles of period length within the duration
fn cycles(duration: Duration, period: Duration) -> f64 {
    let d = 1_000_000_000 * duration.as_secs() as u64 + duration.subsec_nanos() as u64;
    let p = 1_000_000_000 * period.as_secs() as u64 + period.subsec_nanos() as u64;
    d as f64 / p as f64
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "unstable")]
    extern crate test;

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
        let mut r = Builder::new().capacity(1000).build();

        assert_eq!(r.available, 0.0);

        r.give(1.0);
        assert_eq!(r.available, 1.0);

        r.give(1.5);
        assert_eq!(r.available, 2.5);
    }

    #[test]
    fn test_tick() {
        let mut r = Builder::new().capacity(1000).build();
        let t = r.t0;
        r.tick(t + Duration::new(1, 0));
        assert_eq!(r.available, 1.0);
        let t = r.t0;
        r.tick(t + Duration::new(0, 500_000_000));
        assert_eq!(r.available, 1.5);
    }

    #[test]
    fn test_take() {
        let mut r = Builder::new().frequency(1000).capacity(1000).build();
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
    fn block_1_million_per_second_1000ns(b: &mut test::Bencher) {
        let mut r = Builder::new()
            .capacity(10000)
            .frequency(1_000_000)
            .build();
        b.iter(|| r.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_2_million_per_second_500ns(b: &mut test::Bencher) {
        let mut r = Builder::new()
            .capacity(10000)
            .frequency(2_000_000)
            .build();
        b.iter(|| r.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_10_million_per_second_100ns(b: &mut test::Bencher) {
        let mut r = Builder::new()
            .capacity(10000)
            .frequency(10_000_000)
            .build();
        b.iter(|| r.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_20_million_per_second_50ns(b: &mut test::Bencher) {
        let mut r = Builder::new()
            .capacity(10000)
            .frequency(20_000_000)
            .build();
        b.iter(|| r.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_1_million_per_second_1000ns_sync(b: &mut test::Bencher) {
        let r = Builder::new()
            .capacity(10000)
            .frequency(1_000_000)
            .build_sync();
        let h = r.make_handle();
        b.iter(|| h.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_2_million_per_second_500ns_sync(b: &mut test::Bencher) {
        let r = Builder::new()
            .capacity(10000)
            .frequency(2_000_000)
            .build_sync();
        let h = r.make_handle();
        b.iter(|| h.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_10_million_per_second_100ns_sync(b: &mut test::Bencher) {
        let r = Builder::new()
            .capacity(10000)
            .frequency(10_000_000)
            .build_sync();
        let h = r.make_handle();
        b.iter(|| h.wait());
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_20_million_per_second_50ns_sync(b: &mut test::Bencher) {
        let r = Builder::new()
            .capacity(10000)
            .frequency(20_000_000)
            .build_sync();
        let h = r.make_handle();
        b.iter(|| h.wait());
    }
}
