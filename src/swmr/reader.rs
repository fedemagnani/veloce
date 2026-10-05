use std::sync::{Arc, atomic::Ordering};

use crate::swmr::{
    WaitStrategy,
    polling::Polling,
    register::{NO_SLOT, Register, SlotHeader},
};

pub struct Reader<T, W = Polling>
where
    W: WaitStrategy,
{
    pub(super) register: Arc<Register<T, W>>,
    index: usize,
    /// [`SlotHeader`] of the value returned by the last call to [`Reader::latest`]
    pinned: Option<SlotHeader>,
}

impl<T, W> Reader<T, W>
where
    W: WaitStrategy,
{
    pub(super) fn new(register: Arc<Register<T, W>>, index: usize) -> Self {
        Self {
            register,
            index,
            pinned: None,
        }
    }

    /// Returns `true` if the `Writer` committed a value not yet returned by [`Reader::latest`].
    /// A reader that never called [`Reader::latest`] hasn't seen the initial value yet.
    pub fn has_changed(&self) -> bool {
        // no slot is read, so there is nothing to synchronize with
        let current = self.register.current.load(Ordering::Relaxed);
        let current_version = current.version();
        self.version() != Some(current_version)
    }

    /// Returns `true` if the `Writer` has been dropped: the value returned by [`Reader::latest`]
    /// is then the final one, and it stays readable.
    pub fn is_closed(&self) -> bool {
        // no slot is read, so there is nothing to synchronize with
        let current = self.register.current.load(Ordering::Relaxed);
        current.is_closed()
    }

    /// Version of the value returned by the last call to [`Reader::latest`], `None` if never called.
    /// The initial value has version zero, and each commit increments it, wrapping on overflow.
    pub fn version(&self) -> Option<usize> {
        self.pinned.map(SlotHeader::version)
    }

    /// Returns an immutable reference of the latest value published by the `Writer`
    /// The slot stays pinned until the next call, without blocking writer nor other readers.
    pub fn latest(&mut self) -> &T {
        let current = self.pin_current_slot();
        let current_slot = &self.register.slots[current];
        unsafe { &*current_slot.get() }
    }

    /// Returns the index of the slot containing the latest published value.
    fn pin_current_slot(&mut self) -> usize {
        // load the slot info of the latest version of the published data
        let mut current = self.register.current.load(Ordering::Acquire);
        // fast path: no commit happened since the last pin, so the slot we already pinned
        // is still the most recent one. While pinned, the writer cannot stage into it,
        // so its content is unchanged.
        // Versions are compared rather than whole headers, since closing doesn't publish a new value
        if let Some(pinned) = self.pinned
            && pinned.version() == current.version()
        {
            return pinned.slot();
        }
        let hp = self.strategy().hazard();
        loop {
            // mark this slot as busy
            hp.store(current.slot(), Ordering::SeqCst);
            // check if a new value was committed meanwhile; SeqCst, as Acquire may validate a slot the writer is recycling
            let new_current = self.register.current.load(Ordering::SeqCst);
            if new_current.version() == current.version() {
                break;
            }
            current = new_current;
        }
        self.pinned = Some(current);
        current.slot()
    }

    /// The [`WaitStrategy`] owned by this reader, holding its hazard pointer
    pub(super) fn strategy(&self) -> &W {
        &self.register.busy_slots[self.index]
    }
}

impl<T, W> Drop for Reader<T, W>
where
    W: WaitStrategy,
{
    /// Releases the pinned slot, so the writer can recycle it
    fn drop(&mut self) {
        let hp = self.strategy().hazard();
        hp.store(NO_SLOT, Ordering::Release);
    }
}
