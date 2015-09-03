//! A leaky bucket ratelimiter for rust

#![crate_type = "lib"]

#![crate_name = "ratelimit"]

use std::thread;
use std::sync::mpsc;
use std::sync::mpsc::sync_channel;

pub struct Ratelimit {
	capacity: usize,
	refill: isize,
	tx: std::sync::mpsc::SyncSender<()>,
	rx: std::sync::mpsc::Receiver<()>,
}

impl Ratelimit {

	/// create a new ratelimit instance
	///
	/// # Example
	/// ```
	/// # use ratelimit::Ratelimit;
	///
	/// let mut ratelimit = Ratelimit::new(0, 1).unwrap();
	pub fn new(capacity: usize, refill: isize) -> Option<Ratelimit> {

		let (tx, rx) = sync_channel(capacity);

		Some(Ratelimit{
			capacity: capacity,
			refill: refill,
			tx: tx,
			rx: rx,
		})
	}
}

#[test]
fn it_works() {
}
