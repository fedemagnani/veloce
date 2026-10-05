//! Writes of a new value while every reader stays idle: the cost of each write path alone

use test::Bencher;

use super::common::{
    ArcSwap, AtomicCell, Latest, LeftRight, Payload, RwLock, SeqLock, TripleBuffer, Veloce, Watch,
    read_seq,
};

/// Writes once per iteration, the `N` readers having read once and then staying idle
fn write_idle<L: Latest<Payload<S>>, const S: usize, const N: usize>(b: &mut Bencher) {
    let init = Payload::new(0);
    // the readers are kept alive, as some libraries stop accepting writes once they drop
    let (mut writer, mut readers) = L::create::<N>(init);
    // each reader holds a value, as a reader that has read at least once would
    for reader in &mut readers {
        read_seq::<L, S>(reader);
    }

    let mut seq = 0;
    b.iter(|| {
        seq += 1;
        let value = Payload::new(seq);
        L::write(&mut writer, value);
    });
}

macro_rules! bench_write_idle {
    ($($name:ident: $lib:ty, $readers:literal;)*) => {
        paste::paste! {
            $(
                #[bench]
                fn [<$name _p8_r $readers>](b: &mut Bencher) {
                    write_idle::<$lib, 8, $readers>(b);
                }

                #[bench]
                fn [<$name _p4k_r $readers>](b: &mut Bencher) {
                    write_idle::<$lib, 4096, $readers>(b);
                }
            )*
        }
    };
}

bench_write_idle! {
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
