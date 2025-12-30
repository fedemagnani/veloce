//! # Throughput — High-Frequency Trading / Streaming Pipeline
//!
//! **Real-world scenario**: Market data feed where a producer streams prices
//! as fast as possible and consumer must keep up.
//!
//! ```text
//! ┌─────────────┐         ┌─────────────┐
//! │  Producer   │ ──────► │  Consumer   │
//! │ (prices)    │  100K   │ (trading)   │
//! │ Thread 1    │  msgs   │ Thread 2    │
//! └─────────────┘         └─────────────┘
//! ```
//!
//! **Key constraint**: Consumer does minimal work per message (just forwarding).
//! Both threads are CPU-bound and always ready to work.
//!
//! ## Trade-offs
//!
//! | Method | Behavior | Best when |
//! |--------|----------|-----------|
//! | `recv_spin` | Immediate feedback to producer | Consumer is fast (this benchmark) |
//! | `drain()` | Batched feedback, producer may stall | Consumer does real work (see `slow_consumer`) |
//!
//! **Note**: `drain()` is slower here because the fast consumer causes producer stalls.
//! See `slow_consumer` benchmark for scenarios where `drain()` wins.

pub use crossbeam_channel::bounded as crossbeam_bounded;
pub use crossbeam_utils::thread::scope;
pub use std::sync::mpsc::sync_channel as std_sync_channel;
pub use test::Bencher;
pub use veloce::spsc::channel;

pub const BUFFER_SIZE: usize = 1024;
pub const TOTAL_MESSAGES: usize = 100_000;

#[bench]
fn veloce_spin(b: &mut Bencher) {
    let (tx, rx) = channel::<i32, BUFFER_SIZE>();

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        // Producer thread
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
fn veloce_try(b: &mut Bencher) {
    let (tx, rx) = channel::<i32, BUFFER_SIZE>();

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        // Producer thread using try_send with spin
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..TOTAL_MESSAGES {
                    loop {
                        match tx.try_send(i as i32) {
                            Ok(()) => break,
                            Err(veloce::spsc::TrySendErr::Full(_)) => {
                                std::hint::spin_loop();
                            }
                            Err(e) => panic!("{:?}", e),
                        }
                    }
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for _ in 0..TOTAL_MESSAGES {
                loop {
                    match rx.try_recv() {
                        Ok(Some(_)) => break,
                        Ok(None) => std::hint::spin_loop(),
                        Err(e) => panic!("{:?}", e),
                    }
                }
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

/// Uses `drain()` for batch receiving: one acquire-load + one release-store per batch.
///
/// Note: In continuous streaming, drain is typically slower than `recv_spin` because
/// the delayed head commit (on drain drop) causes producer stalls. Drain excels in:
/// - Single-threaded batch processing (see burst benchmark)
/// - Bursty producer patterns where consumer processes between bursts
#[bench]
fn veloce_drain(b: &mut Bencher) {
    const DRAIN_BATCH: usize = 256;

    let (tx, mut rx) = channel::<i32, BUFFER_SIZE>();

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
    let (tx, rx) = crossbeam_bounded::<i32>(BUFFER_SIZE);

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
    let (tx, rx) = std_sync_channel::<i32>(BUFFER_SIZE);

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
