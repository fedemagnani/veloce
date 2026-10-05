//! Construction cost of the writer and `N` readers, dropped within the same iteration

use test::Bencher;

use super::common::{
    ArcSwap, AtomicCell, Latest, LeftRight, P8, RwLock, SeqLock, TripleBuffer, Veloce, Watch,
};

/// Creates and drops a writer with `N` readers per iteration
fn create<L: Latest<P8>, const N: usize>(b: &mut Bencher) {
    b.iter(|| {
        let init = P8::new(0);
        L::create::<N>(init)
    });
}

macro_rules! bench_create {
    ($($name:ident: $lib:ty, $readers:literal;)*) => {
        $(
            #[bench]
            fn $name(b: &mut Bencher) {
                create::<$lib, $readers>(b);
            }
        )*
    };
}

bench_create! {
    veloce_r1: Veloce, 1;
    veloce_r3: Veloce, 3;
    watch_r1: Watch, 1;
    watch_r3: Watch, 3;
    arc_swap_r1: ArcSwap, 1;
    arc_swap_r3: ArcSwap, 3;
    left_right_r1: LeftRight, 1;
    left_right_r3: LeftRight, 3;
    triple_buffer_r1: TripleBuffer, 1;
    seqlock_r1: SeqLock, 1;
    seqlock_r3: SeqLock, 3;
    rwlock_r1: RwLock, 1;
    rwlock_r3: RwLock, 3;
    atomic_cell_r1: AtomicCell, 1;
    atomic_cell_r3: AtomicCell, 3;
}
