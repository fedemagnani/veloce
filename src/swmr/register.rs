use crossbeam_utils::CachePadded;
use std::{
    cell::UnsafeCell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::swmr::{HazardPointer, reader::Reader, writer::Writer};

/// Maximal number of slots of a [`Register`].
pub(super) const MAX_SLOTS: usize = u64::BITS as usize;
const _: () = assert!(
    MAX_SLOTS.is_power_of_two(),
    "MAX_SLOTS must be a power of 2"
);
/// Maximal number of readers of a [`Register`], following the triple-buffer design.
pub(super) const MAX_READERS: usize = MAX_SLOTS - 2;

/// Slot information stored as `version (58 bits) | index (6 bits)`
/// - `version` is used to identify the sequence number of the update (this allows readers to count the number of missed updates)
/// - `index` specifies the location of the current slot. The total number of slots are 64, and 2^6 = 64 so 6 bits are enough to store the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SlotInfo(usize);

impl SlotInfo {
    /// Mask selecting the low bits holding the slot index: since [`MAX_SLOTS`] is a power of 2,
    /// it is the highest slot index
    const SLOT_MASK: usize = MAX_SLOTS - 1;
    /// Number of low bits holding the slot index
    pub(super) const SLOT_BITS: u32 = Self::SLOT_MASK.ilog2() + 1;

    pub(super) fn new(version: usize, slot: usize) -> Self {
        Self((version << Self::SLOT_BITS) | slot)
    }

    pub(super) fn slot(self) -> usize {
        self.0 & Self::SLOT_MASK
    }

    pub(super) fn version(self) -> usize {
        self.0 >> Self::SLOT_BITS
    }

    /// [`SlotInfo`] of the value published in `slot` right after the one described by `self`
    pub(super) fn next(self, slot: usize) -> Self {
        let version = self.version().wrapping_add(1);
        Self::new(version, slot)
    }
}

/// Atomic cell holding a [`SlotInfo`]
pub(super) struct AtomicSlotInfo(AtomicUsize);

impl AtomicSlotInfo {
    pub(super) fn new(info: SlotInfo) -> Self {
        let raw = AtomicUsize::new(info.0);
        Self(raw)
    }

    pub(super) fn load(&self, order: Ordering) -> SlotInfo {
        let raw = self.0.load(order);
        SlotInfo(raw)
    }

    pub(super) fn store(&self, info: SlotInfo, order: Ordering) {
        self.0.store(info.0, order);
    }
}

/// Single-writer-multi-reader registry
/// - `T` is the type shared by the single writer to the multiple readers via this registry
/// - `H` is the hazard pointer type, updated by each reader to identify which slots are currently busy
/// - `B` is the bitmap type, defining an upper bound about the maximal slots and readers available for the registry
///
/// This register requires fixed topology: once constructed, it doesn't admit variations in the readers number
pub(super) struct Register<T, H> {
    /// The [`SlotInfo`] of the latest published value
    pub(super) current: AtomicSlotInfo,
    /// Slots currently being read by each reader.
    /// The lengths of this array is equal to the number of outstanding readers.
    pub(super) busy_slots: Box<[CachePadded<H>]>,
    /// Slots where the actual data is being written.
    /// The length of this vector is at least `read_slots.len() + 2`,
    /// following the triple-buffer design.
    pub(super) slots: Box<[CachePadded<UnsafeCell<T>>]>,
}

impl<T, H> Register<T, H> {
    /// Construct a new [`Register`], supplying the closures needed to construct the initial values
    /// of the hazard pointers and published values
    pub fn new(
        num_slots: usize,
        init_slot: impl Fn(usize) -> T,
        num_readers: usize,
        init_hp: impl Fn(usize) -> H,
    ) -> Self {
        assert!(num_slots >= num_readers + 2, "num_slots < num_readers + 2");

        assert!(num_readers > 0, "num_readers == 0");

        assert!(
            num_slots <= MAX_SLOTS,
            "too many slots: num_slots > {}",
            MAX_SLOTS
        );

        assert!(
            num_readers <= MAX_READERS,
            "too many readers: num_readers > {}",
            MAX_READERS
        );

        let slots = (0..num_slots)
            .map(|i| {
                let inner = init_slot(i);
                let inner = UnsafeCell::new(inner);
                CachePadded::new(inner)
            })
            .collect();

        let busy_slots = (0..num_readers)
            .map(|i| {
                let inner = init_hp(i);
                CachePadded::new(inner)
            })
            .collect();

        // the initial value lives in the first slot
        let initial = SlotInfo::new(0, 0);
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
    pub(super) fn first_available_slot(&self) -> usize
    where
        H: HazardPointer,
    {
        // define the bitmap used to accumulate the busy slots. The LSB is associated with
        // the first slot.
        let mut forbidden: u64 = 0;
        // the current slot is not available, since it has the most recent data published
        // we can load it with relaxed ordering since the writer is the only one which can advance it
        let current = self.current.load(Ordering::Relaxed);
        forbidden |= 1 << current.slot();
        // Scan all the hazard pointers, updating the bitmap
        for hp in &self.busy_slots {
            let busy = hp.slot().load(Ordering::SeqCst);
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

    /// Consume the [`Register`] creating the single [`Writer`] and the multi [`Reader`]s
    pub fn split(self) -> (Writer<T, H>, Vec<Reader<T, H>>) {
        let register = Arc::new(self);

        let writer = Writer::from(register.clone());

        let num_readers = register.busy_slots.len();
        let readers = (0..num_readers)
            .map(|i| Reader::new(register.clone(), i))
            .collect();

        (writer, readers)
    }
}
