//! Burst Benchmarks
//!
//! Measures batched send/recv performance: fill the buffer completely,
//! then drain it completely. Single-threaded.
//!
//! ## What is measured
//!
//! - Sequential write throughput (512 sends with no intervening reads)
//! - Sequential read throughput (512 recvs with no intervening writes)
//! - Memory access patterns during bulk operations
//! - Cache behavior when accessing buffer slots sequentially
//!
//! ## Methodology
//!
//! 1. Channel with [`BUFFER_SIZE`](crate::BUFFER_SIZE) (1024) slots is created once
//! 2. Each iteration:
//!    - **Burst send**: 512 consecutive `send` calls (fills half the buffer)
//!    - **Burst recv**: 512 consecutive `recv` calls (drains the buffer)
//!
//! All operations happen on the same thread. This tests raw operation speed
//! without any synchronization or blocking, simulating batch processing patterns.

use crate::{BUFFER_SIZE, Bencher, channel, crossbeam_bounded, std_sync_channel, test};

const BURST_SIZE: usize = 512;

#[bench]
fn veloce(b: &mut Bencher) {
    let (tx, rx) = channel::<i32, BUFFER_SIZE>();
    b.iter(|| {
        // Burst send
        for i in 0..BURST_SIZE {
            tx.try_send(i as i32).unwrap();
        }
        // Burst receive
        for _ in 0..BURST_SIZE {
            test::black_box(rx.try_recv().unwrap());
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
