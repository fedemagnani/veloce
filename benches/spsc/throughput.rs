//! SPSC Throughput Benchmarks
//!
//! Measures sustained message throughput with producer and consumer on separate
//! threads, using a large buffer ([`BUFFER_SIZE`](crate::BUFFER_SIZE) = 1024).
//!
//! ## What is measured
//!
//! - End-to-end throughput of [`TOTAL_MESSAGES`](crate::TOTAL_MESSAGES) (100,000) messages
//! - Cross-thread synchronization overhead
//! - Cache coherency traffic between cores
//! - Blocking/spinning behavior when buffer is full or empty
//!
//! ## Methodology
//!
//! 1. Channel is created **once** before benchmarking
//! 2. A dedicated **producer thread** is spawned and waits for a start signal
//! 3. On each iteration:
//!    - Main thread signals the producer to start
//!    - Producer sends 100,000 `i32` values as fast as possible
//!    - Main thread (consumer) receives all 100,000 values
//!    - Synchronization ensures the iteration completes before the next
//!
//! ## Variants
//!
//! - `veloce_spin`: Uses `send_spin`/`recv_spin` (busy-wait on full/empty)
//! - `veloce_try`: Uses `try_send`/`try_recv` with manual spin loops
//! - `crossbeam`: Uses blocking `send`/`recv` (parks thread when blocked)
//! - `std_sync`: Uses std's blocking `send`/`recv`

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
