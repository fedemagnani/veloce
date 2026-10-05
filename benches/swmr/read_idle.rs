//! Reads of the latest value with no concurrent writes: the cost of each read path alone

use test::{Bencher, black_box};

use super::common::{
    ArcSwap, AtomicCell, Latest, LeftRight, P4K, P8, P64, RwLock, SeqLock, TripleBuffer, Veloce,
    Watch,
};

/// Reads the latest value once per iteration, the writer staying idle
fn read_idle<L: Latest<T>, T>(b: &mut Bencher, init: T) {
    // the writer is kept alive, as some libraries stop serving reads once it drops
    let (_writer, [mut reader]) = L::create::<1>(init);
    b.iter(|| {
        L::read(&mut reader, |latest| {
            // forces copying libraries to materialize the value they copied out
            black_box(latest);
        })
    });
}

macro_rules! bench_read_idle {
    ($($name:ident: $lib:ty;)*) => {
        paste::paste! {
            $(
                #[bench]
                fn [<$name _p8>](b: &mut Bencher) {
                    let init = P8::new(0);
                    read_idle::<$lib, _>(b, init);
                }

                #[bench]
                fn [<$name _p64>](b: &mut Bencher) {
                    let init = P64::new(0);
                    read_idle::<$lib, _>(b, init);
                }

                #[bench]
                fn [<$name _p4k>](b: &mut Bencher) {
                    let init = P4K::new(0);
                    read_idle::<$lib, _>(b, init);
                }
            )*
        }
    };
}

bench_read_idle! {
    veloce: Veloce;
    watch: Watch;
    arc_swap: ArcSwap;
    left_right: LeftRight;
    triple_buffer: TripleBuffer;
    seqlock: SeqLock;
    rwlock: RwLock;
    atomic_cell: AtomicCell;
}
