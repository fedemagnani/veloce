//! Single-threaded batch processing: fill buffer, then drain completely.
//!
//! Measures raw channel throughput without cross-thread synchronization.
//! Producer and consumer run sequentially on the same thread.
//!
//! Real-world scenarios:
//! - Reading records from a file, then processing the batch
//! - Collecting log entries, then flushing to disk
//! - ETL pipelines with staged processing
//!

use crossbeam_channel::bounded as crossbeam_bounded;
use std::sync::mpsc::sync_channel as std_sync_channel;
use test::Bencher;
use veloce::spsc::channel;

const BUFFER_SIZE: usize = 1024;

const BURST_SIZE: usize = 512;

#[bench]
fn veloce(b: &mut Bencher) {
    let (tx, rx) = channel::<i32, BUFFER_SIZE>();
    b.iter(|| {
        for i in 0..BURST_SIZE {
            tx.try_send(i as i32).unwrap();
        }
        for _ in 0..BURST_SIZE {
            test::black_box(rx.try_recv().unwrap());
        }
    });
}

/// Uses `drain()` to batch-receive all items with a single release-store.
/// This is the ideal use case for drain: single-threaded batch processing.
#[bench]
fn veloce_drain(b: &mut Bencher) {
    let (tx, mut rx) = channel::<i32, BUFFER_SIZE>();
    b.iter(|| {
        for i in 0..BURST_SIZE {
            tx.try_send(i as i32).unwrap();
        }
        for v in rx.drain(BURST_SIZE) {
            test::black_box(v);
        }
    });
}

/// Drain with larger batch to show scalability.
#[bench]
fn veloce_drain_full(b: &mut Bencher) {
    let (tx, mut rx) = channel::<i32, BUFFER_SIZE>();
    b.iter(|| {
        // Fill entire buffer
        for i in 0..BUFFER_SIZE {
            tx.try_send(i as i32).unwrap();
        }
        // Drain all at once
        for v in rx.drain(BUFFER_SIZE) {
            test::black_box(v);
        }
    });
}

#[bench]
fn crossbeam(b: &mut Bencher) {
    let (tx, rx) = crossbeam_bounded::<i32>(BUFFER_SIZE);
    b.iter(|| {
        for i in 0..BURST_SIZE {
            tx.send(i as i32).unwrap();
        }
        for _ in 0..BURST_SIZE {
            test::black_box(rx.recv().unwrap());
        }
    });
}

#[bench]
fn std_sync(b: &mut Bencher) {
    let (tx, rx) = std_sync_channel::<i32>(BUFFER_SIZE);
    b.iter(|| {
        for i in 0..BURST_SIZE {
            tx.send(i as i32).unwrap();
        }
        for _ in 0..BURST_SIZE {
            test::black_box(rx.recv().unwrap());
        }
    });
}
