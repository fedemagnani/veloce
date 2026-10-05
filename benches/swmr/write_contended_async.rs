//! Writes of a new value while async-capable readers either await each change or read nonstop without waiting

use std::{
    sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use futures::executor::block_on;
use test::{Bencher, black_box};

use super::notify::{Notify, Veloce, Watch};

/// Writes once per iteration, while `N` readers await each change and wake up to read it
fn write_contended_await<L: Notify, const N: usize>(b: &mut Bencher) {
    let (mut writer, readers) = L::create::<N>();
    // the readers and the bench thread
    let barrier = Barrier::new(N + 1);
    let barrier = &barrier;

    thread::scope(|s| {
        for mut reader in readers {
            s.spawn(move || {
                barrier.wait();
                block_on(async {
                    while let Some(seq) = L::changed(&mut reader).await {
                        black_box(seq);
                    }
                })
            });
        }

        barrier.wait();
        let mut seq = 0;
        b.iter(|| {
            seq += 1;
            L::write(&mut writer, seq);
        });
        // the readers stop once `changed` reports the writer dropped
        drop(writer);
    });
}

/// Writes once per iteration, while `N` readers read nonstop and never wait, so no commit has a waker to wake
fn write_contended_spin<L: Notify, const N: usize>(b: &mut Bencher) {
    // handles are owned here and only borrowed by the threads, so none drops while others still run
    let (mut writer, mut readers) = L::create::<N>();

    let stop = AtomicBool::new(false);
    let stop = &stop;
    // the readers and the bench thread
    let barrier = Barrier::new(N + 1);
    let barrier = &barrier;

    thread::scope(|s| {
        for reader in &mut readers {
            s.spawn(move || {
                barrier.wait();
                while !stop.load(Ordering::Relaxed) {
                    L::read_seq(reader);
                }
            });
        }

        barrier.wait();
        let mut seq = 0;
        b.iter(|| {
            seq += 1;
            L::write(&mut writer, seq);
        });
        stop.store(true, Ordering::Relaxed);
    });
}

macro_rules! bench_write_contended_async {
    ($($name:ident: $lib:ty, $readers:literal;)*) => {
        paste::paste! {
            $(
                #[bench]
                fn [<$name _await_r $readers>](b: &mut Bencher) {
                    write_contended_await::<$lib, $readers>(b);
                }

                #[bench]
                fn [<$name _spin_r $readers>](b: &mut Bencher) {
                    write_contended_spin::<$lib, $readers>(b);
                }
            )*
        }
    };
}

bench_write_contended_async! {
    veloce: Veloce, 1;
    veloce: Veloce, 3;
    watch: Watch, 1;
    watch: Watch, 3;
}
