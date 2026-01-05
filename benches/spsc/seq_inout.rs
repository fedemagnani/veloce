//! Single-threaded send/receive: measure raw per-operation cost.
//!
//! Producer and consumer are the same thread, so there is no cross-thread
//! synchronization. Establishes the absolute minimum overhead baseline.
//! If your real workload is slower than this, the channel is not your bottleneck.
//!
//! Real-world scenarios:
//! - Actor model with single-threaded mailbox
//! - Event loop with internal message queue
//! - Testing and benchmarking infrastructure

use crossbeam_channel::bounded as crossbeam_bounded;
use flume::bounded as flume_bounded;
use kanal::bounded as kanal_bounded;
use std::sync::mpsc::sync_channel as std_sync_channel;
use test::Bencher;
use veloce::spsc::channel;

const BUFFER_SIZE: usize = 1024;
const BATCH_SIZE: usize = 64;

#[bench]
fn veloce(b: &mut Bencher) {
    let (tx, rx) = channel::<i32, BUFFER_SIZE>();
    b.iter(|| {
        tx.try_send(42).unwrap();
        rx.try_recv().unwrap()
    });
}

/// Batched send/drain to show amortized per-item cost.
/// Dividing the result by BATCH_SIZE gives per-item overhead.
#[bench]
fn veloce_batch(b: &mut Bencher) {
    let (tx, mut rx) = channel::<i32, BUFFER_SIZE>();
    b.iter(|| {
        for i in 0..BATCH_SIZE {
            tx.try_send(i as i32).unwrap();
        }
        for v in rx.drain(BATCH_SIZE) {
            test::black_box(v);
        }
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

#[bench]
fn flume(b: &mut Bencher) {
    let (tx, rx) = flume_bounded::<i32>(BUFFER_SIZE);
    b.iter(|| {
        tx.send(42).unwrap();
        rx.recv().unwrap()
    });
}

#[bench]
fn kanal(b: &mut Bencher) {
    let (tx, rx) = kanal_bounded::<i32>(BUFFER_SIZE);
    b.iter(|| {
        tx.send(42).unwrap();
        rx.recv().unwrap()
    });
}
