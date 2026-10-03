use std::sync::{Arc, atomic::Ordering};

use crate::swmr::{
    HazardPointer,
    register::{NO_SLOT, Register, SlotHeader},
};

pub struct Reader<T, H>
where
    H: HazardPointer,
{
    register: Arc<Register<T, H>>,
    index: usize,
    /// [`SlotHeader`] of the value returned by the last call to [`Reader::latest`]
    pinned: Option<SlotHeader>,
}

impl<T, H> Reader<T, H>
where
    H: HazardPointer,
{
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
        let hp = self.register.busy_slots[self.index].slot();
        loop {
            // mark this slot as busy
            hp.store(current.slot(), Ordering::SeqCst);
            // check if during the atomic-store a new value was committed
            let new_current = self.register.current.load(Ordering::Acquire);
            if new_current.version() == current.version() {
                break;
            }
            current = new_current;
        }
        self.pinned = Some(current);
        current.slot()
    }
}

impl<T, H> Reader<T, H>
where
    H: HazardPointer,
{
    pub fn new(register: Arc<Register<T, H>>, index: usize) -> Self {
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
}

impl<T, H> Drop for Reader<T, H>
where
    H: HazardPointer,
{
    /// Releases the pinned slot, so the writer can recycle it
    fn drop(&mut self) {
        let hp = self.register.busy_slots[self.index].slot();
        hp.store(NO_SLOT, Ordering::Release);
    }
}
