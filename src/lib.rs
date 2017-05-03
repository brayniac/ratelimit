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
//! Construct a new `Ratelimit` and call block between actions. For multi-thread
//! clone the channel sender to pass to the threads, in a separate thread, call
//! run on the `Ratelimit` in a tight loop.
//!
//! # Example
//!
//! Construct `Ratelimit` and use in single and then multi-thread modes
//!
//! ```
//!
//! extern crate ratelimit;
//!
//! use std::thread;
//! use std::sync::mpsc;
//! use std::time::Duration;
//!
//! let mut ratelimit = ratelimit::Ratelimit::configure()
//!     .capacity(1) //number of tokens the bucket will hold
//!     .quantum(1) //add one token per interval
//!     .interval(Duration::new(1, 0)) //add quantum tokens every 1 second
//!     .build();
//!
//! // count down to ignition
//! println!("Count-down....");
//! for i in -10..0 {
//!     println!("T {}", i);
//!     ratelimit.block(1);
//! }
//! println!("Ignition!");
//!
//! // clone the sender from Ratelimit
//! let sender = ratelimit.clone_sender();
//!
//! // create ratelimited threads
//! for i in 0..2 {
//!     let s = sender.clone();
//!     thread::spawn(move || {
//!         for x in 0..5 {
//!             s.send(());
//!             println!(".");
//!         }
//!     });
//! }
//!
//! // run the ratelimiter
//! thread::spawn(move || {
//!     loop {
//!         ratelimit.run();
//!     }
//! });

#![crate_type = "lib"]

#![crate_name = "ratelimit"]

#![cfg_attr(feature = "unstable", feature(test))]

#[cfg(feature = "unstable")]
extern crate test;

extern crate shuteye;


use shuteye::sleep;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub struct Config {
    start: Instant,
    capacity: u32,
    quantum: u32,
    interval: Duration,
}

pub struct Ratelimit {
    config: Config,
    t0: Instant,
    available: f64,
    tx: mpsc::SyncSender<()>,
    rx: mpsc::Receiver<()>,
}

impl Config {
    /// returns a Ratelimit from the Config
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
    ///
    /// let mut r = Ratelimit::configure().build();
    pub fn build(self) -> Ratelimit {
        Ratelimit::configured(self)
    }

    /// sets the number of tokens to add per interval
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
    ///
    /// let mut r = Ratelimit::configure()
    ///     .quantum(100)
    ///     .build();
    pub fn quantum(mut self, quantum: u32) -> Self {
        self.quantum = quantum;
        self
    }

    /// sets the bucket capacity
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
    ///
    /// let mut r = Ratelimit::configure()
    ///     .capacity(100)
    ///     .build();
    pub fn capacity(mut self, capacity: u32) -> Self {
        self.capacity = capacity;
        self
    }

    /// set the duration between token adds
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
    /// # use std::time::Duration;
    ///
    /// let mut r = Ratelimit::configure()
    ///     .interval(Duration::new(2, 0))
    ///     .build();
    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// set the frequency in Hz of the ratelimiter
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
    ///
    /// let mut rate = 100_000; // 100kHz
    /// let mut r = Ratelimit::configure()
    ///     .frequency(rate)
    ///     .build();
    pub fn frequency(mut self, cycles: u32) -> Self {
        let mut interval = Duration::new(1, 0);
        interval /= cycles;
        self.interval = interval;
        self
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            start: Instant::now(),
            capacity: 1,
            quantum: 1,
            interval: Duration::new(1, 0),
        }
    }
}

impl Default for Ratelimit {
    fn default() -> Ratelimit {
        Config::default().build()
    }
}

impl Ratelimit {
    /// create a new default ratelimit instance
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
    ///
    /// let mut r = Ratelimit::new();
    pub fn new() -> Ratelimit {
        Default::default()
    }

    /// create a new `Config` for building a custom `Ratelimit`
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
    ///
    /// let mut r = Ratelimit::configure().build();
    pub fn configure() -> Config {
        Config::default()
    }

    // internal function for building a Ratelimit from its Config
    fn configured(config: Config) -> Ratelimit {
        let (tx, rx) = mpsc::sync_channel(config.capacity as usize);
        let t0 = config.start;
        Ratelimit {
            config: config,
            t0: t0,
            available: 0.0,
            tx: tx,
            rx: rx,
        }
    }

    /// run the ratelimiter
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
    ///
    /// let mut r = Ratelimit::new();
    ///
    /// let sender = r.clone_sender();
    /// let _ = sender.try_send(());
    ///
    /// r.run(); // invoke in a tight-loop in its own thread
    pub fn run(&mut self) {
        let quantum = self.config.quantum;
        self.block(quantum);
        let mut unused = 0;
        for _ in 0..self.config.quantum {
            match self.rx.try_recv() {
                Ok(..) => {}
                Err(_) => {
                    unused += 1;
                }
            }
        }
        self.give(unused as f64);
    }

    /// return clone of SyncSender for client use
    ///
    /// # Example
    /// ```
    /// # use ratelimit::*;
    ///
    /// let mut r = Ratelimit::new();
    ///
    /// let sender = r.clone_sender();
    ///
    /// match sender.try_send(()) {
    ///     Ok(_) => {
    ///         println!("not limited");
    ///     },
    ///     Err(_) => {
    ///         println!("was limited");
    ///     },
    /// }
    pub fn clone_sender(&mut self) -> mpsc::SyncSender<()> {
        self.tx.clone()
    }

    /// this call is the blocking API of Ratelimit
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
    ///
    /// let mut r = Ratelimit::new();
    /// for i in -10..0 {
    ///     println!("T {}...", i);
    ///     r.block(1);
    /// }
    /// println!("Ignition!");
    pub fn block(&mut self, count: u32) {
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
pub fn cycles(duration: Duration, period: Duration) -> f64 {
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

    extern crate shuteye;

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
        let mut r = Ratelimit::configure().capacity(1000).build();

        assert_eq!(r.available, 0.0);

        r.give(1.0);
        assert_eq!(r.available, 1.0);

        r.give(1.5);
        assert_eq!(r.available, 2.5);
    }

    #[test]
    fn test_tick() {
        let mut r = Ratelimit::configure().capacity(1000).build();
        let t = r.t0;
        r.tick(t + Duration::new(1, 0));
        assert_eq!(r.available, 1.0);
        let t = r.t0;
        r.tick(t + Duration::new(0, 500_000_000));
        assert_eq!(r.available, 1.5);
    }

    #[test]
    fn test_take() {
        let mut r = Ratelimit::configure()
            .frequency(1000)
            .capacity(1000)
            .build();
        let t = r.t0;
        assert_eq!(r.take(t + Duration::new(1, 0), 1000), None);
        assert_eq!(r.take(t + Duration::new(1, 0), 1000),
                   Some(Duration::new(1, 0)));
        assert_eq!(r.take(t + Duration::new(2, 0), 1000),
                   Some(Duration::new(1, 0)));
        assert_eq!(r.take(t + Duration::new(4, 0), 1000), None);
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_1_million_per_second_1000ns(b: &mut test::Bencher) {
        let mut r = Ratelimit::configure()
            .capacity(10000)
            .frequency(1_000_000)
            .build();
        b.iter(|| r.block(1));
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_2_million_per_second_500ns(b: &mut test::Bencher) {
        let mut r = Ratelimit::configure()
            .capacity(10000)
            .frequency(2_000_000)
            .build();
        b.iter(|| r.block(1));
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_10_million_per_second_100ns(b: &mut test::Bencher) {
        let mut r = Ratelimit::configure()
            .capacity(10000)
            .frequency(10_000_000)
            .build();
        b.iter(|| r.block(1));
    }

    #[cfg(feature = "unstable")]
    #[bench]
    fn block_20_million_per_second_50ns(b: &mut test::Bencher) {
        let mut r = Ratelimit::configure()
            .capacity(10000)
            .frequency(20_000_000)
            .build();
        b.iter(|| r.block(1));
    }
}
