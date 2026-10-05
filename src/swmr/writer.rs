use std::sync::{Arc, atomic::Ordering};

use crate::swmr::{SwmrError, WaitStrategy, polling::Polling};

use super::register::Register;

pub struct Writer<T, W = Polling>
where
    W: WaitStrategy,
{
    register: Arc<Register<T, W>>,
    staged: Option<usize>,
}

impl<T, W> Writer<T, W>
where
    W: WaitStrategy,
{
    /// Stages `value` and commits it, overriding any value staged and not yet committed
    pub fn publish(&mut self, value: T) -> Result<(), SwmrError> {
        self.update(|slot, _| *slot = value)
    }

    /// Stages via `setter`, which also receives the latest committed value, and commits it
    pub fn update(&mut self, setter: impl FnOnce(&mut T, &T)) -> Result<(), SwmrError> {
        // no reader can ever see the value, so the setter is not even run
        if self.is_disconnected() {
            return Err(SwmrError::Disconnected);
        }
        self.stage_with_latest(setter);
        self.commit()
    }

    /// Uses the closure passed as input in order to set a value in the
    /// available slot. It doesn't publish the value yet.
    pub fn stage(&mut self, setter: impl FnOnce(&mut T)) {
        self.stage_with_latest(|slot, _| setter(slot));
    }

    /// Like [`Writer::stage`], but the closure also receives the latest committed value
    pub fn stage_with_latest(&mut self, setter: impl FnOnce(&mut T, &T)) {
        let staged_idx = self
            .staged
            .unwrap_or_else(|| self.register.first_available_slot());
        // the writer is the only one advancing `current`, so it can load it with relaxed ordering
        let current = self.register.current.load(Ordering::Relaxed);
        // the staged slot is never the current one
        debug_assert_ne!(staged_idx, current.slot());
        // readers only take shared references to the current slot, so the writer can do the same
        let latest = unsafe { &*self.register.slots[current.slot()].get() };
        // in single-writer flavor, the writer has exclusive access
        let slot_mut = unsafe { &mut *self.register.slots[staged_idx].get() };
        setter(slot_mut, latest);
        self.staged = Some(staged_idx);
    }

    /// Updates the slot number containing the most recent data, notifying the readers
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
        // SeqCst: readers store their hazard pointer and waiting flag before re-checking `current`
        self.register.current.store(next, Ordering::SeqCst);
        self.notify_readers();

        Ok(())
    }

    fn is_disconnected(&self) -> bool {
        Arc::strong_count(&self.register) == 1
    }

    /// If [`SealedWaitStrategy::notify`](crate::swmr::sealed::SealedWaitStrategy::notify) no-ops, then LLVM removes the loop completely
    fn notify_readers(&self) {
        for reader in &self.register.busy_slots {
            reader.notify();
        }
    }
}

impl<T, W> Drop for Writer<T, W>
where
    W: WaitStrategy,
{
    /// Marks the latest committed value as final, so readers can detect the disconnection.
    fn drop(&mut self) {
        let mut current = self.register.current.load(Ordering::Relaxed);
        current.set_closed();
        // SeqCst: like a commit, so that waiting readers either observe the closure or get notified
        self.register.current.store(current, Ordering::SeqCst);
        self.notify_readers();
    }
}

impl<T, W> From<Arc<Register<T, W>>> for Writer<T, W>
where
    W: WaitStrategy,
{
    fn from(value: Arc<Register<T, W>>) -> Self {
        Self {
            register: value,
            staged: None,
        }
    }
}
