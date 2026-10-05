//! Adapters of the primitives whose readers can await a value not seen yet

use test::black_box;

use super::common::P8;

/// A single-writer-multi-reader primitive whose readers can await a value not seen yet
pub trait Notify {
    type Writer: Send;
    type Reader: Send;

    /// Creates the writer and `N` readers, all starting from sequence number zero
    fn create<const N: usize>() -> (Self::Writer, [Self::Reader; N]);

    /// Publishes the value carrying `seq`
    fn write(writer: &mut Self::Writer, seq: u64);

    /// Waits for a value not seen yet and returns its sequence number, `None` once the writer drops
    async fn changed(reader: &mut Self::Reader) -> Option<u64>;

    /// Reads the sequence number of the latest value without waiting, marking it as seen
    fn read_seq(reader: &mut Self::Reader) -> u64;
}

/// `veloce::swmr` with async readers
pub struct Veloce;

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
        let seq = Self::read_seq(reader);
        Some(seq)
    }

    fn read_seq(reader: &mut Self::Reader) -> u64 {
        let latest = reader.latest();
        black_box(latest.seq())
    }
}

/// `tokio::sync::watch`
pub struct Watch;

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
        let seq = Self::read_seq(reader);
        Some(seq)
    }

    fn read_seq(reader: &mut Self::Reader) -> u64 {
        let latest = reader.borrow_and_update();
        black_box(latest.seq())
    }
}
