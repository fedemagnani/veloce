//! Payloads and per-library adapters; reads go through a visitor, so only libraries copying out pay for a copy

use std::sync::Arc;

/// Payload of `S` bytes, the first 8 holding a sequence number
#[derive(Clone, Copy)]
#[repr(C, align(8))]
pub struct Payload<const S: usize>([u8; S]);

impl<const S: usize> Payload<S> {
    /// Payload carrying `seq`, the remaining bytes zeroed
    pub fn new(seq: u64) -> Self {
        const { assert!(S >= 8, "payload must fit the sequence number") };
        let mut bytes = [0; S];
        let seq_bytes = seq.to_le_bytes();
        bytes[..8].copy_from_slice(&seq_bytes);
        Self(bytes)
    }

    /// Sequence number carried by the payload
    pub fn seq(&self) -> u64 {
        let mut seq_bytes = [0; 8];
        seq_bytes.copy_from_slice(&self.0[..8]);
        u64::from_le_bytes(seq_bytes)
    }
}

/// Fits a register word
pub type P8 = Payload<8>;
/// Fills a cache line
pub type P64 = Payload<64>;
/// Fills a memory page, where copying out dominates
pub type P4K = Payload<4096>;

/// A single-writer-multi-reader primitive publishing the latest `T`
pub trait Latest<T> {
    type Writer: Send;
    type Reader: Send;

    /// Creates the writer and `N` readers, all starting from `init`
    fn create<const N: usize>(init: T) -> (Self::Writer, [Self::Reader; N]);

    /// Publishes `value`, making it the latest one
    fn write(writer: &mut Self::Writer, value: T);

    /// Runs `f` on the latest value
    fn read<R>(reader: &mut Self::Reader, f: impl FnOnce(&T) -> R) -> R;
}

/// `veloce::swmr` with polling readers
pub struct Veloce;

impl<T: Clone + Send + Sync> Latest<T> for Veloce {
    type Writer = veloce::swmr::Writer<T>;
    type Reader = veloce::swmr::Reader<T>;

    fn create<const N: usize>(init: T) -> (Self::Writer, [Self::Reader; N]) {
        veloce::swmr::register(init)
    }

    fn write(writer: &mut Self::Writer, value: T) {
        writer.publish(value).expect("readers alive");
    }

    fn read<R>(reader: &mut Self::Reader, f: impl FnOnce(&T) -> R) -> R {
        let latest = reader.latest();
        f(latest)
    }
}

/// `tokio::sync::watch`, borrowing under its internal read lock
pub struct Watch;

impl<T: Send + Sync> Latest<T> for Watch {
    type Writer = tokio::sync::watch::Sender<T>;
    type Reader = tokio::sync::watch::Receiver<T>;

    fn create<const N: usize>(init: T) -> (Self::Writer, [Self::Reader; N]) {
        let (tx, rx) = tokio::sync::watch::channel(init);
        let readers = std::array::from_fn(|_| rx.clone());
        (tx, readers)
    }

    fn write(writer: &mut Self::Writer, value: T) {
        writer.send_replace(value);
    }

    fn read<R>(reader: &mut Self::Reader, f: impl FnOnce(&T) -> R) -> R {
        let latest = reader.borrow();
        f(&latest)
    }
}

/// `arc_swap::ArcSwap`, allocating an `Arc` on every write
pub struct ArcSwap;

impl<T: Send + Sync> Latest<T> for ArcSwap {
    type Writer = Arc<arc_swap::ArcSwap<T>>;
    type Reader = Arc<arc_swap::ArcSwap<T>>;

    fn create<const N: usize>(init: T) -> (Self::Writer, [Self::Reader; N]) {
        let shared = Arc::new(arc_swap::ArcSwap::from_pointee(init));
        let readers = std::array::from_fn(|_| shared.clone());
        (shared, readers)
    }

    fn write(writer: &mut Self::Writer, value: T) {
        let value = Arc::new(value);
        writer.store(value);
    }

    fn read<R>(reader: &mut Self::Reader, f: impl FnOnce(&T) -> R) -> R {
        let latest = reader.load();
        f(&latest)
    }
}

/// Copy of the value held by `left_right`
#[derive(Clone)]
pub struct LeftRightCell<T>(T);

/// Operation replacing the value held by `left_right`
pub struct Set<T>(T);

impl<T: Clone> left_right::Absorb<Set<T>> for LeftRightCell<T> {
    fn absorb_first(&mut self, operation: &mut Set<T>, _: &Self) {
        self.0 = operation.0.clone();
    }

    fn absorb_second(&mut self, operation: Set<T>, _: &Self) {
        self.0 = operation.0;
    }

    fn sync_with(&mut self, first: &Self) {
        self.0 = first.0.clone();
    }
}

/// `left_right`, whose writer waits for readers to leave the copy it is about to update
pub struct LeftRight;

impl<T: Clone + Send + Sync> Latest<T> for LeftRight {
    type Writer = left_right::WriteHandle<LeftRightCell<T>, Set<T>>;
    type Reader = left_right::ReadHandle<LeftRightCell<T>>;

    fn create<const N: usize>(init: T) -> (Self::Writer, [Self::Reader; N]) {
        let cell = LeftRightCell(init);
        let (writer, reader) = left_right::new_from_empty(cell);
        let readers = std::array::from_fn(|_| reader.clone());
        (writer, readers)
    }

    fn write(writer: &mut Self::Writer, value: T) {
        let operation = Set(value);
        writer.append(operation).publish();
    }

    fn read<R>(reader: &mut Self::Reader, f: impl FnOnce(&T) -> R) -> R {
        let latest = reader.enter().expect("writer alive");
        f(&latest.0)
    }
}

/// `triple_buffer`, supporting a single reader only
pub struct TripleBuffer;

impl<T: Clone + Send> Latest<T> for TripleBuffer {
    type Writer = triple_buffer::Input<T>;
    type Reader = triple_buffer::Output<T>;

    fn create<const N: usize>(init: T) -> (Self::Writer, [Self::Reader; N]) {
        const { assert!(N == 1, "triple_buffer supports a single reader") };
        let (input, output) = triple_buffer::TripleBuffer::new(&init).split();
        let mut output = Some(output);
        let readers = std::array::from_fn(|_| output.take().expect("single reader"));
        (input, readers)
    }

    fn write(writer: &mut Self::Writer, value: T) {
        writer.write(value);
    }

    fn read<R>(reader: &mut Self::Reader, f: impl FnOnce(&T) -> R) -> R {
        let latest = reader.read();
        f(latest)
    }
}

/// `seqlock::SeqLock`, copying the value out and retrying on concurrent writes
pub struct SeqLock;

impl<T: Copy + Send> Latest<T> for SeqLock {
    type Writer = Arc<seqlock::SeqLock<T>>;
    type Reader = Arc<seqlock::SeqLock<T>>;

    fn create<const N: usize>(init: T) -> (Self::Writer, [Self::Reader; N]) {
        let shared = Arc::new(seqlock::SeqLock::new(init));
        let readers = std::array::from_fn(|_| shared.clone());
        (shared, readers)
    }

    fn write(writer: &mut Self::Writer, value: T) {
        let mut guard = writer.lock_write();
        *guard = value;
    }

    fn read<R>(reader: &mut Self::Reader, f: impl FnOnce(&T) -> R) -> R {
        let latest = reader.read();
        f(&latest)
    }
}

/// `std::sync::RwLock` baseline
pub struct RwLock;

impl<T: Send + Sync> Latest<T> for RwLock {
    type Writer = Arc<std::sync::RwLock<T>>;
    type Reader = Arc<std::sync::RwLock<T>>;

    fn create<const N: usize>(init: T) -> (Self::Writer, [Self::Reader; N]) {
        let shared = Arc::new(std::sync::RwLock::new(init));
        let readers = std::array::from_fn(|_| shared.clone());
        (shared, readers)
    }

    fn write(writer: &mut Self::Writer, value: T) {
        let mut guard = writer.write().expect("not poisoned");
        *guard = value;
    }

    fn read<R>(reader: &mut Self::Reader, f: impl FnOnce(&T) -> R) -> R {
        let latest = reader.read().expect("not poisoned");
        f(&latest)
    }
}

/// `crossbeam_utils::atomic::AtomicCell`, copying out and falling back to a global lock table for large `T`
pub struct AtomicCell;

impl<T: Copy + Send> Latest<T> for AtomicCell {
    type Writer = Arc<crossbeam_utils::atomic::AtomicCell<T>>;
    type Reader = Arc<crossbeam_utils::atomic::AtomicCell<T>>;

    fn create<const N: usize>(init: T) -> (Self::Writer, [Self::Reader; N]) {
        let shared = Arc::new(crossbeam_utils::atomic::AtomicCell::new(init));
        let readers = std::array::from_fn(|_| shared.clone());
        (shared, readers)
    }

    fn write(writer: &mut Self::Writer, value: T) {
        writer.store(value);
    }

    fn read<R>(reader: &mut Self::Reader, f: impl FnOnce(&T) -> R) -> R {
        let latest = reader.load();
        f(&latest)
    }
}

/// Every reader sees the initial value, then each written one
#[cfg(test)]
fn round_trip<L: Latest<P64>, const N: usize>() {
    let init = P64::new(0);
    let (mut writer, mut readers) = L::create::<N>(init);

    for reader in &mut readers {
        let seq = L::read(reader, Payload::seq);
        assert_eq!(seq, 0);
    }

    for v in 1..=10 {
        let value = P64::new(v);
        L::write(&mut writer, value);
        for reader in &mut readers {
            let seq = L::read(reader, Payload::seq);
            assert_eq!(seq, v);
        }
    }
}

#[test]
fn adapters_round_trip() {
    round_trip::<Veloce, 3>();
    round_trip::<Watch, 3>();
    round_trip::<ArcSwap, 3>();
    round_trip::<LeftRight, 3>();
    round_trip::<TripleBuffer, 1>();
    round_trip::<SeqLock, 3>();
    round_trip::<RwLock, 3>();
    round_trip::<AtomicCell, 3>();
}
