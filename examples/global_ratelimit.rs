extern crate ratelimit;

use std::thread;
use std::time::Instant;

pub fn main() {
    let mut limiter = ratelimit::Builder::new().build();
    let handle = limiter.make_handle();

    thread::spawn(move || limiter.run());

    let mut threads = Vec::new();
    for _ in 0..10 {
        let handle = handle.clone();
        threads.push(thread::spawn(move || worker(handle)));
    }
    for thread in threads {
        thread.join().unwrap();
    }
}

pub fn worker(mut handle: ratelimit::Handle) {
    for _ in 0..6 {
        loop {
            if handle.try_wait().is_ok() {
                println!("{:?}", Instant::now());
                break;
            }
        }
    }
}
