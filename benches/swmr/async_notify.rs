//! Latency from a write to every reader awaiting a change, woken and acknowledging the written value

use std::{
    hint::spin_loop,
    sync::atomic::{AtomicU64, Ordering},
    thread,
};

use crossbeam_utils::CachePadded;
use futures::executor::block_on;
use test::Bencher;

use super::notify::{Notify, Veloce, Watch};

/// Writes once per iteration, then waits until each of the `N` readers acknowledges the value
fn async_notify<L: Notify, const N: usize>(b: &mut Bencher) {
    let (mut writer, readers) = L::create::<N>();
    // sequence number last seen by each reader, padded so acknowledgements don't contend
    let acks: [CachePadded<AtomicU64>; N] = std::array::from_fn(|_| {
        let ack = AtomicU64::new(0);
        CachePadded::new(ack)
    });

    thread::scope(|s| {
        for (mut reader, ack) in readers.into_iter().zip(&acks) {
            s.spawn(move || {
                block_on(async {
                    while let Some(seq) = L::changed(&mut reader).await {
                        ack.store(seq, Ordering::Release);
                    }
                })
            });
        }

        let mut seq = 0;
        b.iter(|| {
            seq += 1;
            L::write(&mut writer, seq);
            for ack in &acks {
                while ack.load(Ordering::Acquire) != seq {
                    spin_loop();
                }
            }
        });
        // the readers stop once `changed` reports the writer dropped
        drop(writer);
    });
}

macro_rules! bench_async_notify {
    ($($name:ident: $lib:ty, $readers:literal;)*) => {
        paste::paste! {
            $(
                #[bench]
                fn [<$name _r $readers>](b: &mut Bencher) {
                    async_notify::<$lib, $readers>(b);
                }
            )*
        }
    };
}

bench_async_notify! {
    veloce: Veloce, 1;
    veloce: Veloce, 3;
    watch: Watch, 1;
    watch: Watch, 3;
}
