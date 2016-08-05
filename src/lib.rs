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

extern crate shuteye;

use std::time::{Duration, Instant};
use std::sync::mpsc;

use shuteye::sleep;

pub struct Config {
    start: Instant,
    capacity: u32,
    quantum: u32,
    interval: Duration,
}

pub struct Ratelimit {
    config: Config,
    t0: Instant,
    available: i64,
    tick: u64,
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
            available: 0,
            tick: 0,
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
        self.give(unused);
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
        if self.config.interval == Duration::new(0, 0) {
            return;
        }
        if let Some(ts) = self.take(Instant::now(), count) {
            let _ = sleep(ts);
        }
    }

    // internal function to give back unused tokens
    fn give(&mut self, count: u32) {
        self.available += count as i64;

        if self.available >= self.config.capacity as i64 {
            self.available = self.config.capacity as i64;
        }
    }

    // return time to sleep until tokens are available
    fn take(&mut self, t1: Instant, tokens: u32) -> Option<Duration> {
        if tokens == 0 {
            return None;
        }

        let _ = self.tick(t1);
        let available = self.available - tokens as i64;
        if available >= 0 {
            self.available = available;
            return None;
        }

        let needed_ticks = -available as u32 / self.config.quantum;
        let mut wait_time = self.config.interval * needed_ticks;
        if t1 > self.t0 {
            wait_time -= t1 - self.t0;
        }
        self.available = available;
        Some(wait_time)
    }

    // move the time forward and do bookkeeping
    fn tick(&mut self, t1: Instant) -> u64 {
        let tick = cycles(t1.duration_since(self.t0), self.config.interval) as u64 + self.tick;

        if self.available >= self.config.capacity as i64 {
            return tick;
        }
        if tick == self.tick {
            return tick;
        }

        self.available += (tick as i64 - self.tick as i64) * self.config.quantum as i64;
        if self.available > self.config.capacity as i64 {
            self.available = self.config.capacity as i64;
        }
        self.tick = tick;
        self.t0 = t1;
        tick
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
    use super::Ratelimit;
    use std::time::Duration;

    extern crate shuteye;

    #[test]
    fn test_tick_same() {
        let mut r = Ratelimit::new();
        let t0 = r.t0;

        let tick = r.tick(t0);
        assert_eq!(tick, r.tick(t0));
    }

    #[test]
    fn test_tick_next() {
        let mut r = Ratelimit::new();
        let t0 = r.t0;

        assert_eq!(r.tick(t0), 0);
        assert_eq!(r.tick(t0 + Duration::new(0, 1)), 0);
        assert_eq!(r.tick(t0 + Duration::new(0, 999_999_999)), 0);
        assert_eq!(r.tick(t0 + Duration::new(1, 0)), 1);
        assert_eq!(r.tick(t0 + Duration::new(1, 1)), 1);
        assert_eq!(r.tick(t0 + Duration::new(1, 999_999_999)), 1);
        assert_eq!(r.tick(t0 + Duration::new(2, 0)), 2);
    }

    #[test]
    fn test_take() {
        let mut r = Ratelimit::new();
        let mut t = r.t0;
        t += Duration::new(1, 0);

        assert_eq!(r.take(t, 1), None);

        t += Duration::new(0, 1);
        assert_eq!(r.take(t, 1).unwrap().subsec_nanos(), 999_999_999);
    }
}
