//! Writes of a new value while every reader reads nonstop

use std::{
    sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use test::Bencher;

use super::common::{
    ArcSwap, AtomicCell, Latest, LeftRight, P8, RwLock, SeqLock, TripleBuffer, Veloce, Watch,
    read_until,
};

/// Writes once per iteration on the bench thread, while `N` readers spin
fn write_contended<L: Latest<P8>, const N: usize>(b: &mut Bencher) {
    let init = P8::new(0);
    // handles are owned here and only borrowed by the threads, so none drops while others still run
    let (mut writer, mut readers) = L::create::<N>(init);

    let stop = AtomicBool::new(false);
    let stop = &stop;
    // the readers and the bench thread
    let barrier = Barrier::new(N + 1);
    let barrier = &barrier;

    thread::scope(|s| {
        for reader in &mut readers {
            s.spawn(move || {
                barrier.wait();
                read_until::<L, _>(reader, stop);
            });
        }

        barrier.wait();
        let mut seq = 0;
        b.iter(|| {
            seq += 1;
            let value = P8::new(seq);
            L::write(&mut writer, value);
        });
        stop.store(true, Ordering::Relaxed);
    });
}

macro_rules! bench_write_contended {
    ($($name:ident: $lib:ty, $readers:literal;)*) => {
        paste::paste! {
            $(
                #[bench]
                fn [<$name _p8_r $readers>](b: &mut Bencher) {
                    write_contended::<$lib, $readers>(b);
                }
            )*
        }
    };
}

bench_write_contended! {
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
