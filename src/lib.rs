//! A token bucket ratelimiter for rust

#![crate_type = "lib"]

#![crate_name = "ratelimit"]

extern crate time;

use std::sync::mpsc;
use std::fmt;

const ONE_SECOND: u64 = 1_000_000_000;

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

#[repr(C)]
#[derive(Copy, Clone, PartialEq)]
pub struct timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

impl fmt::Debug for timespec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "timespec(sec: {}, nsec: {})", self.tv_sec, self.tv_nsec)
    }
}

impl Ratelimit {

    /// create a new ratelimit instance
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
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

    /// this should be run in a tight-loop in its own thread
    pub fn run(&mut self) {
        let take = self.quantum;
        self.block(take);
        for _ in 0..take {
            let _ = self.rx.recv();
        }
    }

    /// give a sender for client to use
    pub fn clone_sender(&mut self) -> mpsc::SyncSender<()> {
        self.tx.clone()
    }

    /// block as long as take says to
    fn block(&mut self, count: usize) {
        //let delay = self.take(time::precise_time_ns(), count);
        match self.take(time::precise_time_ns(), count) {
            Some(ts) => {
                let start = time::precise_time_ns();
                self.clock_nanosleep(1, 0, &ts, None);
                let stop = time::precise_time_ns();
                let diff = (stop - start) as i64;
                assert!(diff > ts.tv_nsec);
            },
            None => {},
        }
    }

    // return time to sleep until token is available
    fn take(&mut self, time: u64, count: usize) -> Option<timespec> {

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
        let seconds = (wait_time as f64 / ONE_SECOND as f64).floor() as u64;
        let nano_seconds = wait_time - seconds * ONE_SECOND;

        Some(timespec { tv_sec: seconds as i64, tv_nsec: nano_seconds as i64 })
    }

    /// we need fine-grained sleep
    fn clock_nanosleep(&mut self,
                       id: i32,
                       flags: i32,
                       req: &timespec,
                       remain: Option<&mut timespec>)
                       -> i32 {
        extern {
            fn clock_nanosleep(clock_id: i32,
                               flags: i32,
                               req: *const timespec,
                               rem: *mut timespec)
                               -> i32;
        }
        match remain {
            Some(p) => unsafe { clock_nanosleep(id, flags, req as *const _, p as *mut _) },
            _ => unsafe { clock_nanosleep(id, flags, req as *const _, 0 as *mut _) },
        }
    }

    /// move the time forward and do bookkeeping
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
        assert_eq!(r.take(0, 1), Some(timespec { tv_sec: 0, tv_nsec: 1000 }));
        assert_eq!(r.take(0, 1), Some(timespec { tv_sec: 0, tv_nsec: 2000 }));
        assert_eq!(r.take(1000, 1), Some(timespec { tv_sec: 0, tv_nsec: 2000 }));
        assert_eq!(r.take(3000, 1), Some(timespec { tv_sec: 0, tv_nsec: 1000 }));
        assert_eq!(r.take(5000, 1), None);
    }
}
