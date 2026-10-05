//! Latency from a write to every reader awaiting a change, woken and acknowledging the written value

use std::{
    hint::spin_loop,
    sync::atomic::{AtomicU64, Ordering},
    thread,
};

use crossbeam_utils::CachePadded;
use futures::executor::block_on;
use test::Bencher;

use super::common::P8;

/// A single-writer-multi-reader primitive whose readers can await a value not seen yet
trait Notify {
    type Writer: Send;
    type Reader: Send;

    /// Creates the writer and `N` readers, all starting from sequence number zero
    fn create<const N: usize>() -> (Self::Writer, [Self::Reader; N]);

    /// Publishes the value carrying `seq`
    fn write(writer: &mut Self::Writer, seq: u64);

    /// Waits for a value not seen yet and returns its sequence number, `None` once the writer drops
    async fn changed(reader: &mut Self::Reader) -> Option<u64>;
}

/// `veloce::swmr` with async readers
struct Veloce;

impl Notify for Veloce {
    type Writer = veloce::swmr::AsyncWriter<P8>;
    type Reader = veloce::swmr::AsyncReader<P8>;

    fn create<const N: usize>() -> (Self::Writer, [Self::Reader; N]) {
        let init = P8::new(0);
        veloce::swmr::register_async(init)
    }

    fn write(writer: &mut Self::Writer, seq: u64) {
        let value = P8::new(seq);
        writer.publish(value).expect("readers alive");
    }

    async fn changed(reader: &mut Self::Reader) -> Option<u64> {
        reader.changed().await.ok()?;
        let latest = reader.latest();
        Some(latest.seq())
    }
}

/// `tokio::sync::watch`
struct Watch;

impl Notify for Watch {
    type Writer = tokio::sync::watch::Sender<P8>;
    type Reader = tokio::sync::watch::Receiver<P8>;

    fn create<const N: usize>() -> (Self::Writer, [Self::Reader; N]) {
        let init = P8::new(0);
        let (tx, rx) = tokio::sync::watch::channel(init);
        let readers = std::array::from_fn(|_| rx.clone());
        (tx, readers)
    }

    fn write(writer: &mut Self::Writer, seq: u64) {
        let value = P8::new(seq);
        writer.send_replace(value);
    }

    async fn changed(reader: &mut Self::Reader) -> Option<u64> {
        reader.changed().await.ok()?;
        let latest = reader.borrow_and_update();
        Some(latest.seq())
    }
}

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
