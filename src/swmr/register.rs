use crossbeam_utils::CachePadded;
use std::{
    cell::UnsafeCell,
    sync::{Arc, atomic::AtomicUsize},
};

use crate::swmr::{reader::Reader, writer::Writer};
/// Single-writer-multi-reader registry
/// - `T` is the type shared by the single writer to the multiple readers via this registry
/// - `H` is the hazard pointer type, updated by each reader to identify which slots are currently busy
/// - `B` is the bitmap type, defining an upper bound about the maximal slots and readers available for the registry
///
/// This register requires fixed topology: once constructed, it doesn't admit variations in the readers number
pub(super) struct Register<T, H> {
    /// The slot index containing the latest version of the published data
    pub(super) current: AtomicUsize,
    /// Slots currently being read by each reader.
    /// The lengths of this array is equal to the number of outstanding readers.
    pub(super) busy_slots: Box<[CachePadded<H>]>,
    /// Slots where the actual data is being written.
    /// The length of this vector is at least `read_slots.len() + 2`,
    /// following the triple-buffer design.
    pub(super) slots: Box<[CachePadded<UnsafeCell<T>>]>,
}

impl<T, H> Register<T, H> {
    /// This constraint allows to perform fast scanning of the available slots.
    pub const MAX_SLOTS: usize = 64;
    /// Follows the triple-buffer design.
    pub const MAX_READERS: usize = Self::MAX_SLOTS - 2;

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
            num_slots <= Self::MAX_SLOTS,
            "too many slots: num_slots > {}",
            Self::MAX_SLOTS
        );

        assert!(
            num_readers <= Self::MAX_READERS,
            "too many readers: num_readers > {}",
            Self::MAX_READERS
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

        let current = AtomicUsize::default();

        Self {
            current,
            busy_slots,
            slots,
        }
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
