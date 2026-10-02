use std::sync::{Arc, atomic::Ordering};

use crate::swmr::{HazardPointer, SwmrError};

use super::register::Register;

pub struct Writer<T, H> {
    register: Arc<Register<T, H>>,
    staged: Option<usize>,
}

impl<T, H> Writer<T, H>
where
    H: HazardPointer,
{
    /// Uses the closure passed as input in order to set a value in the
    /// available slot. It doesn't publish the value yet.
    pub fn stage(&mut self, setter: impl Fn(&mut T)) {
        let idx = self.staged.unwrap_or_else(|| self.first_available_slot());
        // in single-writer flavor, the writer has exclusive access
        let slot_mut = unsafe { &mut *self.register.slots[idx].get() };
        setter(slot_mut);
        self.staged = Some(idx);
    }

    /// Updates the slot number containing the most recent data
    pub fn commit(&mut self) -> Result<(), SwmrError> {
        if self.is_disconnected() {
            return Err(SwmrError::Disconnected);
        }
        let Some(idx) = self.staged.take() else {
            return Err(SwmrError::ValueNotStaged);
        };
        // could be ordering release?
        self.register.current.store(idx, Ordering::SeqCst);

        Ok(())
    }

    /// Scan all the hazard pointers to identify which slots are currently read by consumers.
    /// Then, it returns the available slot with lowest index.
    fn first_available_slot(&self) -> usize {
        // define the bitmap used to accumulate the busy slots. The LSB is associated with
        // the first slot.
        let mut forbidden: u64 = 0;
        // the current slot is not available, since it has the most recent data published
        // we can load it with relaxed ordering since the writer is the only one which can advance it
        forbidden |= 1 << self.register.current.load(Ordering::Relaxed);
        // Scan all the hazard pointers, updating the bitmap
        for hp in &self.register.busy_slots {
            let busy = hp.slot().load(Ordering::SeqCst);
            forbidden |= 1 << busy;
        }

        let n_slots = self.register.slots.len();
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

    fn is_disconnected(&self) -> bool {
        Arc::strong_count(&self.register) == 1
    }
}

impl<T, H> From<Arc<Register<T, H>>> for Writer<T, H> {
    fn from(value: Arc<Register<T, H>>) -> Self {
        Self {
            register: value,
            staged: None,
        }
    }
}
