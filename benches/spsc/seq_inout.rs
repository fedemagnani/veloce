//! # Sequential In/Out — Raw Operation Cost
//!
//! **Real-world scenario**: Actor model where a single thread owns both ends
//! of the channel. Measures the absolute minimum overhead per operation.
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │           Same Thread                   │
//! │  send(msg) ──► [buffer] ──► recv()      │
//! │                                         │
//! │  No cross-thread sync needed            │
//! │  Best-case cache locality               │
//! └─────────────────────────────────────────┘
//! ```
//!
//! **Use case**: Establishing a baseline. If your real workload is slower
//! than this, the channel isn't your bottleneck.
//!
//! ## Variants
//!
//! | Benchmark | What it measures |
//! |-----------|------------------|
//! | `veloce` | Single send + recv (~1.7ns) |
//! | `veloce_batch` | 64 items with drain (~112ns total, ~1.75ns/item) |

use crossbeam_channel::bounded as crossbeam_bounded;
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
