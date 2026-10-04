//! [`Polling`] strategy: readers poll for new values, the writer never notifies them

use std::sync::atomic::AtomicUsize;

use super::{WaitStrategy, reader::Reader, register::NO_SLOT, sealed::Sealed, writer::Writer};

#[cfg(test)]
mod test;

/// [`Reader`] polling for new values
pub type PollingReader<T> = Reader<T, Polling>;

/// [`Writer`] of a register whose readers poll for new values
pub type PollingWriter<T> = Writer<T, Polling>;

/// Readers poll [`Reader::has_changed`], the writer never notifies them
pub struct Polling {
    hazard: AtomicUsize,
}

impl Default for Polling {
    /// The strategy of a reader not pinning any slot
    fn default() -> Self {
        let hazard = AtomicUsize::new(NO_SLOT);
        Self { hazard }
    }
}

impl Sealed for Polling {}

impl WaitStrategy for Polling {
    fn hazard(&self) -> &AtomicUsize {
        &self.hazard
    }
}
