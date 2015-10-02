//! A token bucket ratelimiter for rust

#![crate_type = "lib"]

#![crate_name = "ratelimit"]

extern crate time;

use std::sync::mpsc;

pub struct Ratelimit {
    capacity: usize,
    interval: u64,
    ticks: u64,
    last_tick: u64,
    correction: u64,
    tx: mpsc::SyncSender<()>,
    rx: mpsc::Receiver<()>,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

impl Ratelimit {

    /// create a new ratelimit instance
    ///
    /// # Example
    /// ```
    /// # use ratelimit::Ratelimit;
    ///
    /// let mut r = Ratelimit::new(0, 1).unwrap();
    pub fn new(capacity: usize, interval: u64) -> Option<Ratelimit> {

        let (tx, rx) = mpsc::sync_channel(capacity);

        Some(Ratelimit {
            capacity: capacity,
            interval: interval,
            ticks: 0_u64,
            last_tick: time::precise_time_ns(),
            correction: 0_u64,
            tx: tx,
            rx: rx,
        })
    }

    /// this should be run in a tight-loop in its own thread
    pub fn run(&mut self) {
        self.block(1);
        let _ = self.rx.recv();
    }

    /// give a sender for client to use
    pub fn clone_sender(&mut self) -> mpsc::SyncSender<()> {
        self.tx.clone()
    }

    /// block until can take
    fn block(&mut self, count: usize) {
        loop {
            self.tick();
            if self.capacity >= count {
                self.capacity -= count;
                break;
            }

            self.clock_nanosleep(1, 0, &timespec { tv_sec: 0, tv_nsec: 1000 }, None);
        }
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
    fn tick(&mut self) {
        let this_tick = time::precise_time_ns();
        let interval = this_tick - self.last_tick;

        if interval > self.interval {
            let increment = (interval as f64 / self.interval as f64).floor() as u64;
            self.correction += interval - (increment * self.interval);
            self.ticks += increment;
            self.capacity += increment as usize;
            self.last_tick = this_tick;
        }

        if self.correction > self.interval {
            let increment = (self.correction as f64 / self.interval as f64).floor() as u64;
            self.correction -= increment * self.interval;
            self.ticks += increment;
            self.capacity += increment as usize;
            self.last_tick = this_tick;
        }
    }
}
