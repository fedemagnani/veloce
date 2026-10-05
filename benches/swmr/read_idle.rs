//! Reads of the latest value with no concurrent writes: the cost of each read path alone

use test::Bencher;

use super::common::{
    ArcSwap, AtomicCell, Latest, LeftRight, Payload, RwLock, SeqLock, TripleBuffer, Veloce, Watch,
    read_seq,
};

/// Reads the latest value once per iteration, the writer staying idle
fn read_idle<L: Latest<Payload<S>>, const S: usize>(b: &mut Bencher) {
    let init = Payload::new(0);
    // the writer is kept alive, as some libraries stop serving reads once it drops
    let (_writer, [mut reader]) = L::create::<1>(init);
    b.iter(|| read_seq::<L, S>(&mut reader));
}

macro_rules! bench_read_idle {
    ($($name:ident: $lib:ty;)*) => {
        paste::paste! {
            $(
                #[bench]
                fn [<$name _p8>](b: &mut Bencher) {
                    read_idle::<$lib, 8>(b);
                }

                #[bench]
                fn [<$name _p64>](b: &mut Bencher) {
                    read_idle::<$lib, 64>(b);
                }

                #[bench]
                fn [<$name _p4k>](b: &mut Bencher) {
                    read_idle::<$lib, 4096>(b);
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
