//! A triple-buffer inspired wait-free, lock-free single-writer-multi-reader primitive
//!
//! Useful for publishing data from a single writer to an audience of multiple readers
//! interested only in the latest version of the published data.

use std::sync::atomic::AtomicUsize;

mod reader;
mod register;
#[cfg(test)]
mod test;
mod writer;

#[derive(Debug)]
pub enum SwmrError {
    Disconnected,
    ValueNotStaged,
}

trait HazardPointer {
    fn slot(&self) -> &AtomicUsize;
}

impl HazardPointer for AtomicUsize {
    fn slot(&self) -> &AtomicUsize {
        self
    }
}
