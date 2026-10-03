use std::sync::{Arc, atomic::Ordering};

use crate::swmr::{
    HazardPointer,
    register::{NO_SLOT, Register, SlotInfo},
};

pub struct Reader<T, H>
where
    H: HazardPointer,
{
    register: Arc<Register<T, H>>,
    index: usize,
    /// [`SlotInfo`] of the value returned by the last call to [`Reader::latest`]
    pinned: Option<SlotInfo>,
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
        // fast path: the slot we already pinned is still the most recent one.
        // While pinned, the writer cannot stage into it, so its content is unchanged.
        if self.pinned == Some(current) {
            return current.slot();
        }
        let hp = self.register.busy_slots[self.index].slot();
        loop {
            // mark this slot as busy
            hp.store(current.slot(), Ordering::SeqCst);
            // check if during the atomic-store the value changed
            let new_current = self.register.current.load(Ordering::Acquire);
            if new_current == current {
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
        self.pinned != Some(current)
    }

    /// Version of the value returned by the last call to [`Reader::latest`], `None` if never called.
    /// The initial value has version zero, and each commit increments it, wrapping on overflow.
    pub fn version(&self) -> Option<usize> {
        self.pinned.map(SlotInfo::version)
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
