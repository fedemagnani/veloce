//! Round trip of a value published by the bench thread and echoed back by a reader thread through a second instance

use std::{
    hint::spin_loop,
    sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use test::Bencher;

use super::common::{
    ArcSwap, AtomicCell, Latest, LeftRight, Payload, RwLock, SeqLock, TripleBuffer, Veloce, Watch,
    read_seq,
};

/// Spins until `reader` sees `seq`, returning `false` if `stop` is set first
fn wait_for<L: Latest<Payload<S>>, const S: usize>(
    reader: &mut L::Reader,
    seq: u64,
    stop: &AtomicBool,
) -> bool {
    loop {
        let seen = read_seq::<L, S>(reader);
        if seen == seq {
            return true;
        }
        let stopped = stop.load(Ordering::Relaxed);
        if stopped {
            return false;
        }
        spin_loop();
    }
}

/// Publishes once per iteration on `ping`, then waits for the echo thread to publish it back on `pong`
fn propagation<L: Latest<Payload<S>>, const S: usize>(b: &mut Bencher) {
    // handles are owned here and only borrowed by the threads, so none drops while others still run
    let (mut ping_writer, [mut ping_reader]) = L::create::<1>(Payload::new(0));
    let (mut pong_writer, [mut pong_reader]) = L::create::<1>(Payload::new(0));

    let stop = AtomicBool::new(false);
    let stop = &stop;
    // the echo thread and the bench thread
    let barrier = Barrier::new(2);
    let barrier = &barrier;
    let ping_reader = &mut ping_reader;
    let pong_writer = &mut pong_writer;

    thread::scope(|s| {
        s.spawn(move || {
            barrier.wait();
            let mut expected = 1;
            while wait_for::<L, S>(ping_reader, expected, stop) {
                let echo = Payload::new(expected);
                L::write(pong_writer, echo);
                expected += 1;
            }
        });

        barrier.wait();
        let mut seq = 0;
        b.iter(|| {
            seq += 1;
            let ping = Payload::new(seq);
            L::write(&mut ping_writer, ping);
            wait_for::<L, S>(&mut pong_reader, seq, stop);
        });
        stop.store(true, Ordering::Relaxed);
    });
}

macro_rules! bench_propagation {
    ($($name:ident: $lib:ty;)*) => {
        paste::paste! {
            $(
                #[bench]
                fn [<$name _p8>](b: &mut Bencher) {
                    propagation::<$lib, 8>(b);
                }

                #[bench]
                fn [<$name _p4k>](b: &mut Bencher) {
                    propagation::<$lib, 4096>(b);
                }
            )*
        }
    };
}

bench_propagation! {
    veloce: Veloce;
    watch: Watch;
    arc_swap: ArcSwap;
    left_right: LeftRight;
    triple_buffer: TripleBuffer;
    seqlock: SeqLock;
    rwlock: RwLock;
    atomic_cell: AtomicCell;
}
