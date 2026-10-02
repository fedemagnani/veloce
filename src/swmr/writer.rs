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
        let idx = self
            .staged
            .unwrap_or_else(|| self.register.first_available_slot());
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
        // the writer is the only one advancing `current`, so it can load it with relaxed ordering
        let current = self.register.current.load(Ordering::Relaxed);
        let next = current.next(idx);
        // could be ordering release?
        self.register.current.store(next, Ordering::SeqCst);

        Ok(())
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
