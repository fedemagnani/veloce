//! Oneshot Benchmarks
//!
//! Measures the full round-trip cost of a single message: channel creation,
//! one send, and one receive.
//!
//! ## What is measured
//!
//! - Channel allocation and initialization
//! - Single `send` operation (non-blocking for veloce, blocking for others)
//! - Single `recv` operation
//! - Channel teardown (implicit drop)
//!
//! ## Methodology
//!
//! Each iteration:
//! 1. Creates a new channel with [`BUFFER_SIZE`](crate::BUFFER_SIZE) capacity
//! 2. Sends a single `i32` value
//! 3. Receives and returns the value
//!
//! This simulates request-response patterns where channels are short-lived.

use crate::{BUFFER_SIZE, Bencher, channel, crossbeam_bounded, std_sync_channel};

#[bench]
fn veloce(b: &mut Bencher) {
    b.iter(|| {
        let (tx, rx) = channel::<i32, BUFFER_SIZE>();
        tx.try_send(42).unwrap();
        rx.try_recv().unwrap()
    });
}

#[bench]
fn crossbeam(b: &mut Bencher) {
    b.iter(|| {
        let (tx, rx) = crossbeam_bounded::<i32>(BUFFER_SIZE);
        tx.send(42).unwrap();
        rx.recv().unwrap()
    });
}

#[bench]
fn std_sync(b: &mut Bencher) {
    b.iter(|| {
        let (tx, rx) = std_sync_channel::<i32>(BUFFER_SIZE);
        tx.send(42).unwrap();
        rx.recv().unwrap()
    });
}
