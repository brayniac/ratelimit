//! A token bucket ratelimiter for rust

#![crate_type = "lib"]

#![crate_name = "ratelimit"]

extern crate time;
extern crate shuteye;

use std::sync::mpsc;
use shuteye::*;

pub struct Ratelimit {
    start: u64,
    capacity: isize,
    interval: u64,
    available: isize,
    tick: u64,
    quantum: usize,
    tx: mpsc::SyncSender<()>,
    rx: mpsc::Receiver<()>,
}

impl Ratelimit {

    /// create a new ratelimit instance
    ///
    /// # Example
    /// ```
    /// # use ratelimit::*;
    ///
    /// let mut r = Ratelimit::new(0, 0, 1, 1).unwrap();
    pub fn new(capacity: usize, start: u64, interval: u64, quantum: usize) -> Option<Ratelimit> {

        let (tx, rx) = mpsc::sync_channel(capacity);

        Some(Ratelimit {
            start: start,
            capacity: capacity as isize,
            interval: interval,
            available: capacity as isize, // needs to go negative
            quantum: quantum,
            tick: 0_u64,
            tx: tx,
            rx: rx,
        })
    }

    /// run the ratelimiter
    ///
    /// # Example
    /// ```
    /// # use ratelimit::*;
    ///
    /// let mut r = Ratelimit::new(1, 0, 1, 1).unwrap();
    ///
    /// r.run(); // invoke in a tight-loop in its own thread
    pub fn run(&mut self) {
        let take = self.quantum;
        self.block(take);
        for _ in 0..take {
            let _ = self.rx.try_recv();
        }
    }

    /// return clone of SyncSender for client use
    ///
    /// # Example
    /// ```
    /// # use ratelimit::*;
    ///
    /// let mut r = Ratelimit::new(1, 0, 1, 1).unwrap();
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

    // block as long as take says to
    fn block(&mut self, count: usize) {
        match self.take(time::precise_time_ns(), count) {
            Some(ts) => {
                let _ = shuteye::sleep(ts);
            },
            None => {},
        }
    }

    // return time to sleep until token is available
    fn take(&mut self, time: u64, count: usize) -> Option<Timespec> {
        if count == 0 {
            return None;
        }

        let _ = self.tick(time);
        let available = self.available - count as isize;
        if available >= 0 {
            self.available = available;
            return None;
        }
        let needed_ticks =
            ((-1 * available + self.quantum as isize - 1) as f64 / (self.quantum as f64)) as u64;
        let wait_time = needed_ticks * self.interval;
        self.available = available;
        match Timespec::from_nano(wait_time as i64) {
            Err(_) => {
                panic!("error getting Timespec from wait_time");
            },
            Ok(ts) => {
                Some(ts)
            }
        }
    }

    // move the time forward and do bookkeeping
    fn tick(&mut self, now: u64) -> u64 {
        //let tick: u64 = (now - self.start) / self.interval;
        let tick: u64 = ((now - self.start) as f64 / self.interval as f64).floor() as u64;

        if self.available >= self.capacity {
            return tick;
        }
        self.available += ((tick - self.tick) * self.quantum as u64) as isize;
        if self.available > self.capacity {
            self.available = self.capacity as isize;
        }
        self.tick = tick;
        tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate time;
    extern crate shuteye;

    #[test]
    fn test_tick_same() {
        let mut r = Ratelimit::new(1, 0, 1000, 1).unwrap();

        let time = time::precise_time_ns();
        let tick = r.tick(time);
        assert_eq!(tick, r.tick(time));
    }

    #[test]
    fn test_tick_next() {
        let mut r = Ratelimit::new(1, 0, 1000, 1).unwrap();

        assert_eq!(r.tick(0), 0);
        assert_eq!(r.tick(1), 0);
        assert_eq!(r.tick(500), 0);
        assert_eq!(r.tick(999), 0);
        assert_eq!(r.tick(1000), 1);
        assert_eq!(r.tick(1001), 1);
        assert_eq!(r.tick(1999), 1);
        assert_eq!(r.tick(2000), 2);
        assert_eq!(r.tick(2001), 2);
        assert_eq!(r.tick(2999), 2);
    }

    #[test]
    fn test_take() {
        let mut r = Ratelimit::new(1, 0, 1000, 1).unwrap();

        assert_eq!(r.take(0, 1), None);
        assert_eq!(r.take(0, 1).unwrap().as_nsec(), 1000);
        assert_eq!(r.take(0, 1).unwrap().as_nsec(), 2000);
        assert_eq!(r.take(1000, 1).unwrap().as_nsec(), 2000);
        assert_eq!(r.take(3000, 1).unwrap().as_nsec(), 1000);
        assert_eq!(r.take(5000, 1), None);
    }
}
