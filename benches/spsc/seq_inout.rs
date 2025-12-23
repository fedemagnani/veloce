//! Sequential In/Out Benchmarks
//!
//! Measures the per-operation cost of send/recv on a pre-allocated channel,
//! with both operations on the same thread.
//!
//! ## What is measured
//!
//! - Pure send operation overhead (no contention, no allocation)
//! - Pure recv operation overhead
//! - Cache effects when producer and consumer share the same core
//!
//! ## Methodology
//!
//! 1. Channel is created **once** before the benchmark loop
//! 2. Each iteration sends one `i32`, then immediately receives it
//! 3. Same thread acts as both producer and consumer
//!
//! This isolates the raw channel operation cost without thread synchronization
//! overhead or channel allocation. Best-case scenario for cache locality.

use crate::{channel, crossbeam_bounded, std_sync_channel, Bencher, BUFFER_SIZE};

#[bench]
fn veloce(b: &mut Bencher) {
    let (tx, rx) = channel::<i32, BUFFER_SIZE>();
    b.iter(|| {
        tx.try_send(42).unwrap();
        rx.try_recv().unwrap()
    });
}

#[bench]
fn crossbeam(b: &mut Bencher) {
    let (tx, rx) = crossbeam_bounded::<i32>(BUFFER_SIZE);
    b.iter(|| {
        tx.send(42).unwrap();
        rx.recv().unwrap()
    });
}

#[bench]
fn std_sync(b: &mut Bencher) {
    let (tx, rx) = std_sync_channel::<i32>(BUFFER_SIZE);
    b.iter(|| {
        tx.send(42).unwrap();
        rx.recv().unwrap()
    });
}

