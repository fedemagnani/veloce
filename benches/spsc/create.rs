//! Channel Creation Benchmarks
//!
//! Measures the time to allocate and initialize a channel with a buffer of
//! [`BUFFER_SIZE`](crate::BUFFER_SIZE) (1024) slots.
//!
//! ## What is measured
//!
//! - Memory allocation for the ring buffer
//! - Initialization of atomic indices and synchronization primitives
//! - Arc creation (for veloce/crossbeam) or internal structures (for std)
//!
//! ## Methodology
//!
//! Each iteration creates a fresh channel. The returned sender/receiver handles
//! are immediately dropped after creation.

use crossbeam_channel::bounded as crossbeam_bounded;
use std::sync::mpsc::sync_channel as std_sync_channel;
use test::Bencher;
use veloce::spsc::channel;
const BUFFER_SIZE: usize = 1024;

#[bench]
fn veloce(b: &mut Bencher) {
    b.iter(channel::<i32, BUFFER_SIZE>);
}

#[bench]
fn crossbeam(b: &mut Bencher) {
    b.iter(|| crossbeam_bounded::<i32>(BUFFER_SIZE));
}

#[bench]
fn std_sync(b: &mut Bencher) {
    b.iter(|| std_sync_channel::<i32>(BUFFER_SIZE));
}
