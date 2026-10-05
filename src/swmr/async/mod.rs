//! [`Async`] strategy: readers await new values, the writer wakes them on commit

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use futures::task::AtomicWaker;

use super::{
    WaitStrategy, reader::Reader, register::NO_SLOT, sealed::SealedWaitStrategy, writer::Writer,
};

mod reader;
#[cfg(test)]
mod test;

pub use reader::Changed;

/// [`Reader`] awaiting new values
pub type AsyncReader<T> = Reader<T, Async>;

/// [`Writer`] of a register whose readers await new values
pub type AsyncWriter<T> = Writer<T, Async>;

/// Readers can await new values with [`Reader::changed`]
pub struct Async {
    hazard: AtomicUsize,
    /// Set while the reader awaits, so that commits skip the waker of readers not waiting
    waiting: AtomicBool,
    waker: AtomicWaker,
}

impl Default for Async {
    /// The strategy of a reader not pinning any slot, nor waiting
    fn default() -> Self {
        let hazard = AtomicUsize::new(NO_SLOT);
        let waiting = AtomicBool::new(false);
        let waker = AtomicWaker::new();
        Self {
            hazard,
            waiting,
            waker,
        }
    }
}

impl SealedWaitStrategy for Async {
    fn hazard(&self) -> &AtomicUsize {
        &self.hazard
    }

    fn notify(&self) {
        // SeqCst pairs with the reader's re-check; only loaded, since the reader clears it
        let waiting = self.waiting.load(Ordering::SeqCst);
        if waiting {
            self.waker.wake();
        }
    }
}

impl WaitStrategy for Async {}
