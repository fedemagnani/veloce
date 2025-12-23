//! Small Buffer Throughput Benchmarks
//!
//! Measures throughput with a small buffer (64 slots) to stress-test
//! backpressure handling when the producer frequently outruns the consumer.
//!
//! ## What is measured
//!
//! - Throughput under high contention (buffer frequently full)
//! - Cost of blocking/spinning when producer must wait for space
//! - Efficiency of wakeup mechanisms when space becomes available
//!
//! ## Methodology
//!
//! Same as [`throughput`](super::throughput) benchmarks, but with `SMALL_BUFFER = 64`:
//!
//! 1. Channel with only 64 slots is created once
//! 2. Producer thread sends [`TOTAL_MESSAGES`](crate::TOTAL_MESSAGES) (100,000) values
//! 3. Consumer receives all values
//! 4. Producer will block/spin ~1,500 times waiting for buffer space
//!
//! This benchmark penalizes implementations with expensive blocking operations
//! and rewards efficient spin-wait or adaptive backoff strategies.

use crate::{channel, crossbeam_bounded, scope, std_sync_channel, Bencher, TOTAL_MESSAGES};

const SMALL_BUFFER: usize = 64;

#[bench]
fn veloce_spin(b: &mut Bencher) {
    let (tx, rx) = channel::<i32, SMALL_BUFFER>();

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..TOTAL_MESSAGES {
                    tx.send_spin(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for _ in 0..TOTAL_MESSAGES {
                rx.recv_spin().unwrap();
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

#[bench]
fn crossbeam(b: &mut Bencher) {
    let (tx, rx) = crossbeam_bounded::<i32>(SMALL_BUFFER);

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..TOTAL_MESSAGES {
                    tx.send(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for _ in 0..TOTAL_MESSAGES {
                rx.recv().unwrap();
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

#[bench]
fn std_sync(b: &mut Bencher) {
    let (tx, rx) = std_sync_channel::<i32>(SMALL_BUFFER);

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..TOTAL_MESSAGES {
                    tx.send(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for _ in 0..TOTAL_MESSAGES {
                rx.recv().unwrap();
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

