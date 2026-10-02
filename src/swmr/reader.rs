use std::sync::{Arc, atomic::Ordering};

use crate::swmr::{HazardPointer, register::Register};

pub struct Reader<T, H> {
    register: Arc<Register<T, H>>,
    index: usize,
}

impl<T, H> Reader<T, H>
where
    H: HazardPointer,
{
    /// Returns an immutable reference of the latest value published by the `Writer`
    /// The slot stays pinned until the next call, without blocking writer nor other readers.
    pub fn latest(&mut self) -> &T {
        // load the index of the slot containing the latest version of the published data
        let mut current = self.register.current.load(Ordering::Acquire);
        loop {
            // mark this slot as busy
            self.register.busy_slots[self.index]
                .slot()
                .store(current, Ordering::SeqCst);
            // check if during the atomic-store the value changed
            let new_current = self.register.current.load(Ordering::Acquire);
            if new_current == current {
                break;
            }
            current = new_current;
        }
        // load the slot containing the latest value
        unsafe { &*self.register.slots[current].get() }
    }
}

impl<T, H> Reader<T, H> {
    pub fn new(register: Arc<Register<T, H>>, index: usize) -> Self {
        Self { register, index }
    }
}
