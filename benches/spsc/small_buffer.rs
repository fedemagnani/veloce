//! Throughput with minimal buffer: stress-test backpressure handling.
//!
//! Uses a small 64-slot buffer where the producer frequently outruns the
//! consumer. Tests how efficiently the channel handles buffer-full conditions
//! and producer stalls.
//!
//! Real-world scenarios:
//! - Embedded systems with limited RAM
//! - IoT sensor pipelines with constrained memory
//! - Rate-limited APIs with small request queues

use crossbeam_channel::bounded as crossbeam_bounded;
use crossbeam_utils::thread::scope;
use flume::bounded as flume_bounded;
use kanal::bounded as kanal_bounded;
use std::sync::mpsc::sync_channel as std_sync_channel;
use test::Bencher;
use veloce::spsc::channel;

const TOTAL_MESSAGES: usize = 100_000;
const SMALL_BUFFER: usize = 64;
const DRAIN_BATCH: usize = 32;

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

/// Drain with small buffer: delayed head commit causes more producer stalls.
/// Expected to be slower than spin due to backpressure.
#[bench]
fn veloce_drain(b: &mut Bencher) {
    let (tx, mut rx) = channel::<i32, SMALL_BUFFER>();

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

            let mut received = 0;
            while received < TOTAL_MESSAGES {
                let drain = rx.drain(DRAIN_BATCH);
                if drain.remaining() == 0 {
                    std::hint::spin_loop();
                    continue;
                }
                for v in drain {
                    test::black_box(v);
                    received += 1;
                }
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

#[bench]
fn flume(b: &mut Bencher) {
    let (tx, rx) = flume_bounded::<i32>(SMALL_BUFFER);

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
fn kanal(b: &mut Bencher) {
    let (tx, rx) = kanal_bounded::<i32>(SMALL_BUFFER);

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
