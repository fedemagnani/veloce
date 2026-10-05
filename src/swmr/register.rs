use crossbeam_utils::CachePadded;
use std::{
    cell::UnsafeCell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::swmr::{WaitStrategy, reader::Reader, writer::Writer};

/// Maximal number of slots of a [`Register`].
pub(super) const MAX_SLOTS: usize = u64::BITS as usize;
const _: () = assert!(
    MAX_SLOTS.is_power_of_two(),
    "MAX_SLOTS must be a power of 2"
);
/// Maximal number of readers of a [`Register`], following the triple-buffer design.
pub(super) const MAX_READERS: usize = MAX_SLOTS - 2;
/// Hazard pointer value of a reader not pinning any slot
pub(super) const NO_SLOT: usize = usize::MAX;

/// Slot information stored as `version (57 bits) | closed (1 bit) | index (6 bits)`
/// - `version` is used to identify the sequence number of the update (this allows readers to count the number of missed updates)
/// - `closed` is set once the [`Writer`] is dropped. Since it lives in the same word as `index`, a reader observing it
///   knows that the slot loaded alongside holds the final value.
/// - `index` specifies the location of the current slot. The total number of slots are 64, and 2^6 = 64 so 6 bits are enough to store the index.
///
/// The `closed` bit sits below `version`, so that a wrapping `version` is truncated by the shift instead of overflowing into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SlotHeader(usize);

impl SlotHeader {
    /// Mask selecting the low bits holding the slot index: since [`MAX_SLOTS`] is a power of 2,
    /// it is the highest slot index
    const SLOT_MASK: usize = MAX_SLOTS - 1;
    /// Number of low bits holding the slot index
    const SLOT_BITS: u32 = Self::SLOT_MASK.ilog2() + 1;
    /// Bit set once the [`Writer`] is dropped
    const CLOSED: usize = 1 << Self::SLOT_BITS;
    /// Number of low bits preceding the version
    pub(super) const VERSION_SHIFT: u32 = Self::SLOT_BITS + 1;

    pub(super) fn new(version: usize, slot: usize) -> Self {
        Self((version << Self::VERSION_SHIFT) | slot)
    }

    pub(super) fn slot(self) -> usize {
        self.0 & Self::SLOT_MASK
    }

    pub(super) fn version(self) -> usize {
        self.0 >> Self::VERSION_SHIFT
    }

    pub(super) fn is_closed(self) -> bool {
        self.0 & Self::CLOSED != 0
    }

    /// Marks the [`SlotHeader`] as closed, leaving version and slot index untouched
    pub(super) fn set_closed(&mut self) {
        self.0 |= Self::CLOSED;
    }

    /// [`SlotHeader`] of the value published in `slot` right after the one described by `self`
    pub(super) fn next(self, slot: usize) -> Self {
        let version = self.version().wrapping_add(1);
        Self::new(version, slot)
    }
}

/// Atomic cell holding a [`SlotHeader`]
pub(super) struct AtomicSlotInfo(AtomicUsize);

// `#[inline]` lets other crates resolve `order` at compile time, instead of calling out and branching on it
impl AtomicSlotInfo {
    #[inline]
    pub(super) fn new(info: SlotHeader) -> Self {
        let raw = AtomicUsize::new(info.0);
        Self(raw)
    }

    #[inline]
    pub(super) fn load(&self, order: Ordering) -> SlotHeader {
        let raw = self.0.load(order);
        SlotHeader(raw)
    }

    #[inline]
    pub(super) fn store(&self, info: SlotHeader, order: Ordering) {
        self.0.store(info.0, order);
    }
}

/// Single-writer-multi-reader registry
/// - `T` is the type shared by the single writer to the multiple readers via this registry
/// - `W` is the [`WaitStrategy`] of the readers, holding the hazard pointer of each reader
///
/// This register requires fixed topology: once constructed, it doesn't admit variations in the readers number
pub(super) struct Register<T, W> {
    /// The [`SlotHeader`] of the latest published value
    pub(super) current: AtomicSlotInfo,
    /// Slots currently being read by each reader.
    /// The lengths of this array is equal to the number of outstanding readers.
    pub(super) busy_slots: Box<[CachePadded<W>]>,
    /// Slots where the actual data is being written.
    /// The length of this vector is exactly `busy_slots.len() + 2`, following the triple-buffer design:
    pub(super) slots: Box<[CachePadded<UnsafeCell<T>>]>,
}

// SAFETY: the hazard pointer protocol guarantees that a slot is never mutated while it is shared.
// - `T: Sync` since multiple readers (and the writer) hold `&T` to the same slot concurrently
// - `T: Send` since the writer mutates slots, and drops them, from a thread other than the readers
// - `W: Sync` since readers store their hazard pointers while the writer scans them
unsafe impl<T: Send + Sync, W: Sync> Sync for Register<T, W> {}

impl<T, W> Register<T, W>
where
    W: WaitStrategy,
{
    /// Construct a new [`Register`] with `num_readers + 2` slots, initialized by `init_slot`
    pub fn new(num_readers: usize, mut init_slot: impl FnMut() -> T) -> Self {
        assert!(num_readers > 0, "num_readers == 0");

        assert!(
            num_readers <= MAX_READERS,
            "too many readers: num_readers > {}",
            MAX_READERS
        );

        let num_slots = num_readers + 2;
        let slots = (0..num_slots)
            .map(|_| {
                let inner = init_slot();
                let inner = UnsafeCell::new(inner);
                CachePadded::new(inner)
            })
            .collect();

        // readers start without pinning any slot
        let busy_slots = (0..num_readers)
            .map(|_| {
                let inner = W::default();
                CachePadded::new(inner)
            })
            .collect();

        // the initial value lives in the first slot
        let initial = SlotHeader::new(0, 0);
        let current = AtomicSlotInfo::new(initial);

        Self {
            current,
            busy_slots,
            slots,
        }
    }

    /// Scan all the hazard pointers to identify which slots are currently read by consumers.
    /// Then, it returns the available slot with lowest index.
    ///
    /// Must be called by the single [`Writer`] only, since it is the one advancing `current`.
    pub(super) fn first_available_slot(&self) -> usize {
        // define the bitmap used to accumulate the busy slots. The LSB is associated with
        // the first slot.
        let mut forbidden: u64 = 0;
        // the current slot is not available, since it has the most recent data published
        // we can load it with relaxed ordering since the writer is the only one which can advance it
        let current = self.current.load(Ordering::Relaxed);
        forbidden |= 1 << current.slot();
        // Scan all the hazard pointers, updating the bitmap
        for hp in &self.busy_slots {
            let busy = hp.hazard().load(Ordering::SeqCst);
            if busy == NO_SLOT {
                continue;
            }
            forbidden |= 1 << busy;
        }

        let n_slots = self.slots.len();
        let mask = if n_slots == u64::BITS as usize {
            // 111..111
            u64::MAX
        } else {
            // 000100...000 -> 000011...111
            (1 << n_slots) - 1
        };
        let available = !forbidden;
        // from available, we set to zero all the bits which are > slots.len()
        let avaiblable = available & mask;

        // starting from LSB, count how many bits before the first one
        avaiblable.trailing_zeros() as usize
    }

    /// Consume the [`Register`] creating the single [`Writer`] and its `N` [`Reader`]s
    pub fn split<const N: usize>(self) -> (Writer<T, W>, [Reader<T, W>; N]) {
        let num_readers = self.busy_slots.len();
        assert_eq!(N, num_readers, "N != num_readers");

        let register = Arc::new(self);

        let writer = Writer::from(register.clone());

        let readers = std::array::from_fn(|i| Reader::new(register.clone(), i));

        (writer, readers)
    }
}
