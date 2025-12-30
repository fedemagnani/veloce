//! # Slow Consumer — JSON Parsing / Database Writes / Crypto
//!
//! **Real-world scenario**: Consumer does actual work per message — parsing JSON,
//! writing to database, computing hashes, or any CPU-intensive processing.
//!
//! ```text
//! ┌─────────────┐               ┌─────────────────────────────┐
//! │  Producer   │ ────────────► │  Consumer                   │
//! │  (fast)     │               │  parse JSON (~50ns)         │
//! │             │               │  validate (~100ns)          │
//! │             │               │  write to DB (~500ns)       │
//! └─────────────┘               └─────────────────────────────┘
//! ```
//!
//! **Key insight**: When consumer is the bottleneck, producer stalls don't matter.
//! The producer stays ahead anyway, and `drain()` saves atomic operations.
//!
//! ## Results Summary
//!
//! | Work per item | `drain()` vs others |
//! |---------------|---------------------|
//! | ~50ns (light) | **9× faster** than `recv_spin` |
//! | ~200ns (medium) | **2× faster** than `recv_spin` |
//!
//! ## When to use `drain()`
//!
//! ✅ Consumer does real work (parsing, I/O, computation)
//! ✅ Consumer is the bottleneck
//! ❌ Consumer is trivial (just forwarding) — use `recv_spin`

use crossbeam_channel::bounded as crossbeam_bounded;
use crossbeam_utils::thread::scope;
use std::sync::mpsc::sync_channel as std_sync_channel;
use test::Bencher;
use veloce::spsc::channel;

const BUFFER_SIZE: usize = 1024;
const ITEMS_PER_ITER: usize = 10_000;

/// Light work: ~50ns per item (where atomics still matter)
#[inline(never)]
fn light_work(v: i32) -> i32 {
    let mut result = v;
    for i in 0..20 {
        result = result.wrapping_add(i);
        std::hint::black_box(result);
    }
    result
}

/// Medium work: ~200ns per item
#[inline(never)]
fn medium_work(v: i32) -> i32 {
    let mut result = v;
    for i in 0..80 {
        result = result.wrapping_add(i);
        std::hint::black_box(result);
    }
    result
}

// ==================== Light Work (~50ns/item) ====================

#[bench]
fn light_spin(b: &mut Bencher) {
    let (tx, rx) = channel::<i32, BUFFER_SIZE>();

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..ITEMS_PER_ITER {
                    tx.send_spin(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for _ in 0..ITEMS_PER_ITER {
                let v = rx.recv_spin().unwrap();
                test::black_box(light_work(v));
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

#[bench]
fn light_drain(b: &mut Bencher) {
    let (tx, mut rx) = channel::<i32, BUFFER_SIZE>();

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..ITEMS_PER_ITER {
                    tx.send_spin(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();

            let mut received = 0;
            while received < ITEMS_PER_ITER {
                let drain = rx.drain(128);
                if drain.remaining() == 0 {
                    std::hint::spin_loop();
                    continue;
                }
                for v in drain {
                    test::black_box(light_work(v));
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
fn light_crossbeam(b: &mut Bencher) {
    let (tx, rx) = crossbeam_bounded::<i32>(BUFFER_SIZE);

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..ITEMS_PER_ITER {
                    tx.send(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for _ in 0..ITEMS_PER_ITER {
                let v = rx.recv().unwrap();
                test::black_box(light_work(v));
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

#[bench]
fn light_std(b: &mut Bencher) {
    let (tx, rx) = std_sync_channel::<i32>(BUFFER_SIZE);

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..ITEMS_PER_ITER {
                    tx.send(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for _ in 0..ITEMS_PER_ITER {
                let v = rx.recv().unwrap();
                test::black_box(light_work(v));
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

// ==================== Medium Work (~200ns/item) ====================

#[bench]
fn medium_spin(b: &mut Bencher) {
    let (tx, rx) = channel::<i32, BUFFER_SIZE>();

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..ITEMS_PER_ITER {
                    tx.send_spin(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for _ in 0..ITEMS_PER_ITER {
                let v = rx.recv_spin().unwrap();
                test::black_box(medium_work(v));
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

#[bench]
fn medium_drain(b: &mut Bencher) {
    let (tx, mut rx) = channel::<i32, BUFFER_SIZE>();

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..ITEMS_PER_ITER {
                    tx.send_spin(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();

            let mut received = 0;
            while received < ITEMS_PER_ITER {
                let drain = rx.drain(128);
                if drain.remaining() == 0 {
                    std::hint::spin_loop();
                    continue;
                }
                for v in drain {
                    test::black_box(medium_work(v));
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
fn medium_crossbeam(b: &mut Bencher) {
    let (tx, rx) = crossbeam_bounded::<i32>(BUFFER_SIZE);

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..ITEMS_PER_ITER {
                    tx.send(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for _ in 0..ITEMS_PER_ITER {
                let v = rx.recv().unwrap();
                test::black_box(medium_work(v));
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}

#[bench]
fn medium_std(b: &mut Bencher) {
    let (tx, rx) = std_sync_channel::<i32>(BUFFER_SIZE);

    let (start_tx, start_rx) = crossbeam_bounded(0);
    let (done_tx, done_rx) = crossbeam_bounded(0);

    scope(|s| {
        s.spawn(|_| {
            while start_rx.recv().is_ok() {
                for i in 0..ITEMS_PER_ITER {
                    tx.send(i as i32).unwrap();
                }
                done_tx.send(()).unwrap();
            }
        });

        b.iter(|| {
            start_tx.send(()).unwrap();
            for _ in 0..ITEMS_PER_ITER {
                let v = rx.recv().unwrap();
                test::black_box(medium_work(v));
            }
            done_rx.recv().unwrap();
        });

        drop(start_tx);
    })
    .unwrap();
}
