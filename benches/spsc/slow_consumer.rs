//! Consumer-bottlenecked pipeline: consumer does real work per message.
//!
//! Simulates workloads where the consumer performs CPU-intensive processing
//! (parsing, validation, computation) rather than just forwarding. The producer
//! stays ahead of the consumer, making backpressure irrelevant.
//!
//! Real-world scenarios:
//! - JSON/XML parsing pipelines
//! - Database write batching
//! - Cryptographic hash computation
//! - Image/video frame processing

use crossbeam_channel::bounded as crossbeam_bounded;
use crossbeam_utils::thread::scope;
use flume::bounded as flume_bounded;
use kanal::bounded as kanal_bounded;
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

#[bench]
fn light_flume(b: &mut Bencher) {
    let (tx, rx) = flume_bounded::<i32>(BUFFER_SIZE);

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
fn light_kanal(b: &mut Bencher) {
    let (tx, rx) = kanal_bounded::<i32>(BUFFER_SIZE);

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

#[bench]
fn medium_flume(b: &mut Bencher) {
    let (tx, rx) = flume_bounded::<i32>(BUFFER_SIZE);

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
fn medium_kanal(b: &mut Bencher) {
    let (tx, rx) = kanal_bounded::<i32>(BUFFER_SIZE);

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
