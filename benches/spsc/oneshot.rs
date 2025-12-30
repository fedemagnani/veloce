//! # Oneshot — Futures / Promises / Task Results
//!
//! **Real-world scenario**: Spawning an async task and waiting for its result.
//! Channel is created, used once, then discarded.
//!
//! ```text
//! let (tx, rx) = channel();
//! spawn(async move {
//!     let result = compute_something().await;
//!     tx.send(result);           // ← one send
//! });
//! let result = rx.recv();        // ← one recv
//! // channel dropped
//! ```
//!
//! **What matters**: Total cost including allocation. If you're creating
//! thousands of oneshot channels per second, this benchmark matters.
//!
//! ## Trade-offs
//!
//! veloce is **~15× faster** than crossbeam/std here because:
//! - Compile-time buffer size (no runtime allocation decisions)
//! - Simpler internal structure (SPSC vs MPMC)

use crossbeam_channel::bounded as crossbeam_bounded;
use std::sync::mpsc::sync_channel as std_sync_channel;
use test::Bencher;
use veloce::spsc::channel;

const BUFFER_SIZE: usize = 1024;
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
