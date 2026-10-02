use std::sync::{Arc, atomic::Ordering};

use crate::swmr::{HazardPointer, register::Register};

const NOT_PINNED: usize = usize::MAX;

pub struct Reader<T, H> {
    register: Arc<Register<T, H>>,
    index: usize,
    pinned: usize,
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
        // load the index of the slot containing the latest version of the published data
        let mut current = self.register.current.load(Ordering::Acquire);
        // fast path: the slot we already pinned is still the most recent one.
        // While pinned, the writer cannot stage into it, so its content is unchanged.
        if self.pinned == current {
            return current;
        }
        let hp = self.register.busy_slots[self.index].slot();
        loop {
            // mark this slot as busy
            hp.store(current, Ordering::SeqCst);
            // check if during the atomic-store the value changed
            let new_current = self.register.current.load(Ordering::Acquire);
            if new_current == current {
                break;
            }
            current = new_current;
        }
        self.pinned = current;
        current
    }
}

impl<T, H> Reader<T, H> {
    pub fn new(register: Arc<Register<T, H>>, index: usize) -> Self {
        Self {
            register,
            index,
            pinned: NOT_PINNED,
        }
    }
}
