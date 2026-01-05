//! Ping-pong latency: measure round-trip time for request-response patterns.
//!
//! Each iteration sends a message and waits for a response before proceeding.
//! Measures single-message latency rather than throughput. Critical for
//! latency-sensitive applications.
//!
//! Real-world scenarios:
//! - Database query round-trips
//! - RPC and microservice communication
//! - Any request-response protocol
//!
//! Note: std creates fresh channels each iteration due to API constraints,
//! so its numbers include allocation overhead.

use crossbeam_channel::bounded as crossbeam_bounded;
use crossbeam_utils::thread::scope;
use flume::bounded as flume_bounded;
use kanal::bounded as kanal_bounded;
use test::Bencher;
use veloce::spsc::lamport::channel;

const PING_PONG_ROUNDS: usize = 10_000;

#[bench]
fn veloce(b: &mut Bencher) {
    let (tx1, rx1) = channel::<i32, 2>();
    let (tx2, rx2) = channel::<i32, 2>();

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        // Pong thread
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for _ in 0..PING_PONG_ROUNDS {
                    let v = rx1.recv_spin().unwrap();
                    tx2.send_spin(v).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        // Ping thread (benchmark thread)
        b.iter(|| {
            start_tx.send(()).unwrap();
            for i in 0..PING_PONG_ROUNDS {
                tx1.send_spin(i as i32).unwrap();
                test::black_box(rx2.recv_spin().unwrap());
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

#[bench]
fn crossbeam(b: &mut Bencher) {
    let (tx1, rx1) = crossbeam_bounded::<i32>(2);
    let (tx2, rx2) = crossbeam_bounded::<i32>(2);

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for _ in 0..PING_PONG_ROUNDS {
                    let v = rx1.recv().unwrap();
                    tx2.send(v).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for i in 0..PING_PONG_ROUNDS {
                tx1.send(i as i32).unwrap();
                test::black_box(rx2.recv().unwrap());
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

/// Note: std::sync::mpsc::Receiver is not Sync, so we cannot share it across threads
/// like we do with veloce and crossbeam. This benchmark creates fresh channels each
/// iteration, which adds overhead but is the only way to do proper pingpong with std.
#[bench]
fn std_sync(b: &mut Bencher) {
    use std::sync::mpsc::channel as std_channel;

    b.iter(|| {
        let (tx1, rx1) = std_channel::<i32>();
        let (tx2, rx2) = std_channel::<i32>();

        let handle = std::thread::spawn(move || {
            for _ in 0..PING_PONG_ROUNDS {
                let v = rx1.recv().unwrap();
                tx2.send(v).unwrap();
            }
        });

        for i in 0..PING_PONG_ROUNDS {
            tx1.send(i as i32).unwrap();
            test::black_box(rx2.recv().unwrap());
        }

        handle.join().unwrap();
    });
}

#[bench]
fn flume(b: &mut Bencher) {
    let (tx1, rx1) = flume_bounded::<i32>(2);
    let (tx2, rx2) = flume_bounded::<i32>(2);

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for _ in 0..PING_PONG_ROUNDS {
                    let v = rx1.recv().unwrap();
                    tx2.send(v).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for i in 0..PING_PONG_ROUNDS {
                tx1.send(i as i32).unwrap();
                test::black_box(rx2.recv().unwrap());
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

#[bench]
fn kanal(b: &mut Bencher) {
    let (tx1, rx1) = kanal_bounded::<i32>(2);
    let (tx2, rx2) = kanal_bounded::<i32>(2);

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for _ in 0..PING_PONG_ROUNDS {
                    let v = rx1.recv().unwrap();
                    tx2.send(v).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for i in 0..PING_PONG_ROUNDS {
                tx1.send(i as i32).unwrap();
                test::black_box(rx2.recv().unwrap());
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}
