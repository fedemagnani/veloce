//! Reads of the latest value while the writer publishes nonstop and the other readers read nonstop

use std::{
    sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use test::{Bencher, black_box};

use super::common::{
    ArcSwap, AtomicCell, Latest, LeftRight, Payload, RwLock, SeqLock, TripleBuffer, Veloce, Watch,
    read_until, write_until,
};

/// Reads once per iteration on the bench thread, while the writer and the other `N - 1` readers spin
fn read_contended<L: Latest<Payload<S>>, const S: usize, const N: usize>(b: &mut Bencher) {
    let init = Payload::new(0);
    // handles are owned here and only borrowed by the threads, so none drops while others still run
    let (mut writer, mut readers) = L::create::<N>(init);
    let (measured, background) = readers.split_first_mut().expect("at least one reader");

    let stop = AtomicBool::new(false);
    let stop = &stop;
    // the writer, the background readers and the bench thread
    let barrier = Barrier::new(N + 1);
    let barrier = &barrier;
    let writer = &mut writer;

    thread::scope(|s| {
        s.spawn(move || {
            barrier.wait();
            write_until::<L, S>(writer, stop);
        });
        for reader in background {
            s.spawn(move || {
                barrier.wait();
                read_until::<L, _>(reader, stop);
            });
        }

        barrier.wait();
        b.iter(|| {
            L::read(measured, |latest| {
                black_box(latest);
            })
        });
        stop.store(true, Ordering::Relaxed);
    });
}

macro_rules! bench_read_contended {
    ($($name:ident: $lib:ty, $readers:literal;)*) => {
        paste::paste! {
            $(
                #[bench]
                fn [<$name _p8_r $readers>](b: &mut Bencher) {
                    read_contended::<$lib, 8, $readers>(b);
                }

                #[bench]
                fn [<$name _p64_r $readers>](b: &mut Bencher) {
                    read_contended::<$lib, 64, $readers>(b);
                }

                #[bench]
                fn [<$name _p4k_r $readers>](b: &mut Bencher) {
                    read_contended::<$lib, 4096, $readers>(b);
                }
            )*
        }
    };
}

bench_read_contended! {
    veloce: Veloce, 1;
    veloce: Veloce, 3;
    watch: Watch, 1;
    watch: Watch, 3;
    arc_swap: ArcSwap, 1;
    arc_swap: ArcSwap, 3;
    left_right: LeftRight, 1;
    left_right: LeftRight, 3;
    triple_buffer: TripleBuffer, 1;
    seqlock: SeqLock, 1;
    seqlock: SeqLock, 3;
    rwlock: RwLock, 1;
    rwlock: RwLock, 3;
    atomic_cell: AtomicCell, 1;
    atomic_cell: AtomicCell, 3;
}
