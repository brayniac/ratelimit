//! A leaky bucket ratelimiter for rust

#![crate_type = "lib"]

#![crate_name = "ratelimit"]


extern crate time;

use std::thread;
use std::sync::mpsc;
use std::sync::mpsc::sync_channel;

// isn't this actually a bucket?
pub struct Ratelimit {
	capacity: u64,
	interval: u64,
	ticks: u64,
	last_tick: u64,
	correction: u64,
}

impl Ratelimit {

	/// create a new ratelimit instance
	///
	/// # Example
	/// ```
	/// # use ratelimit::Ratelimit;
	///
	/// let mut r = Ratelimit::new(0, 1).unwrap();
	pub fn new(capacity: u64, interval: u64) -> Option<Ratelimit> {

		Some(Ratelimit{
			capacity: capacity,
			interval: interval,
			ticks: 0_u64,
			last_tick: time::precise_time_ns(),
			correction: 0_u64,
		})
	}

	/// block until can take
	pub fn take(&mut self, count: u64) {

		self.tick();

		if self.capacity < count {
			std::thread::sleep_ms(1);
			self.take(count);
		}
		else {
			self.capacity -= count;
		}
	}

	fn tick(&mut self) {
		let this_tick = time::precise_time_ns();
		let interval = this_tick - self.last_tick;

		if interval > self.interval {
			let increment = (interval as f64 / self.interval as f64).floor() as u64;
			let error = interval - (increment * self.interval);
			self.correction += error;
			self.ticks += increment;
			self.capacity += increment;
			self.last_tick = this_tick;
		}

		if self.correction > self.interval {
			let increment = (self.correction as f64 / self.interval as f64).floor() as u64;
			let error = self.correction - (increment * self.interval);
			self.correction = error;
			self.ticks += increment;
			self.capacity += increment;
			self.last_tick = this_tick;
		}
	}
}
