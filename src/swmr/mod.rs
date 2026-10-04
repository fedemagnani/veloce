//! A triple-buffer inspired single-writer-multi-reader primitive
//!
//! Useful for publishing data from a single writer to an audience of multiple readers
//! interested only in the latest version of the published data.
//!
//! How readers wait for new values is selected by the [`WaitStrategy`] type parameter:
//! - [`Polling`] (default): readers poll [`Reader::has_changed`]
//! - [`Async`] (`async` feature): readers can await [`Reader::changed`]
//!
//! ## Progress guarantees
//!
//! |                   | [`Polling`] | [`Async`]  |
//! |-------------------|-------------|------------|
//! | Writer wait-free  | yes         | no\*       |
//! | Writer lock-free  | yes         | no\*       |
//! | Readers wait-free | no\*\*      | no\*\*     |
//! | Readers lock-free | yes         | no\*\*\*   |
//!
//! \* waking the readers awaiting [`Reader::changed`] runs executor code, which may lock
//!
//! \*\* [`Reader::latest`] retries pinning as long as the writer keeps committing meanwhile
//!
//! \*\*\* polling [`Reader::changed`] registers the task's waker, which runs executor code

#[cfg(feature = "async")]
mod r#async;
mod polling;
mod reader;
mod register;
mod writer;

mod sealed {
    use std::sync::atomic::AtomicUsize;

    /// Internals of a [`WaitStrategy`](super::WaitStrategy), hidden from the public API
    pub trait SealedWaitStrategy {
        /// Hazard pointer of the reader. Must return the same atomic on every call.
        fn hazard(&self) -> &AtomicUsize;

        /// Called by the writer after each commit, and when it is dropped.
        fn notify(&self) {}
    }
}

#[cfg(feature = "async")]
pub use r#async::{Async, AsyncReader, AsyncWriter, Changed};
pub use polling::{Polling, PollingReader, PollingWriter};
pub use reader::Reader;
pub use writer::Writer;

/// How a [`Reader`] waits for new values: either [`Polling`] or `Async`
pub trait WaitStrategy: sealed::SealedWaitStrategy + Sync + Default {}

#[derive(Debug, PartialEq, Eq)]
pub enum SwmrError {
    Disconnected,
    ValueNotStaged,
}
