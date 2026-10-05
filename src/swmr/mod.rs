//! A triple-buffer inspired single-writer-multi-reader primitive
//!
//! Useful for publishing data from a single writer to an audience of multiple readers
//! interested only in the latest version of the published data.
//!
//! ## Example
//!
//! ```rust
//! use std::thread;
//! use veloce::swmr::{self, SwmrError};
//!
//! // one writer and two readers, all starting from an empty `Vec`
//! let (mut writer, readers) = swmr::register::<Vec<u64>, 2>(Vec::new());
//!
//! let handles = readers.map(|mut reader| {
//!     thread::spawn(move || {
//!         // spin until the writer drops, then borrow its final value without copying it
//!         while !reader.is_closed() {
//!             std::hint::spin_loop();
//!         }
//!         let latest = reader.latest();
//!         latest.iter().sum::<u64>()
//!     })
//! });
//!
//! for v in 1..=10 {
//!     // build each value from the latest one, reusing the allocation of the recycled slot
//!     writer.update(|slot, latest| {
//!         slot.clone_from(latest);
//!         slot.push(v);
//!     })?;
//! }
//! drop(writer);
//!
//! for handle in handles {
//!     let sum = handle.join().unwrap();
//!     assert_eq!(sum, 55);
//! }
//! # Ok::<(), SwmrError>(())
//! ```
//!
//! ## Wait strategies
//!
//! How readers wait for new values is selected by the [`WaitStrategy`] type parameter:
//! - [`Polling`] (default): readers poll [`Reader::has_changed`]
//! - [`Async`] (`async` feature): readers can await [`Reader::changed`], see [`register_async`]
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
//!
// async-only items are linked only when the `async` feature is enabled, otherwise they point to the strategies above
#![cfg_attr(feature = "async", doc = "[`Async`]: Async")]
#![cfg_attr(feature = "async", doc = "[`Reader::changed`]: AsyncReader::changed")]
#![cfg_attr(feature = "async", doc = "[`register_async`]: register_async")]
#![cfg_attr(not(feature = "async"), doc = "[`Async`]: #wait-strategies")]
#![cfg_attr(not(feature = "async"), doc = "[`Reader::changed`]: #wait-strategies")]
#![cfg_attr(not(feature = "async"), doc = "[`register_async`]: #wait-strategies")]

#[cfg(feature = "async")]
mod r#async;
mod polling;
mod reader;
mod register;
#[cfg(test)]
mod test;
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

use register::{MAX_READERS, Register};

/// How a [`Reader`] waits for new values: either [`Polling`] or `Async`
pub trait WaitStrategy: sealed::SealedWaitStrategy + Sync + Default {}

/// Creates a register with `N` [`Polling`] readers, every slot starting as a clone of `initial`
pub fn register<T: Clone, const N: usize>(initial: T) -> (Writer<T>, [Reader<T>; N]) {
    register_with(|| initial.clone())
}

/// Creates a register with `N` readers awaiting new values, every slot starting as a clone of `initial`
///
/// ```rust
/// use std::thread;
/// use futures::executor::block_on;
/// use veloce::swmr::{self, SwmrError};
///
/// let (mut writer, [mut reader]) = swmr::register_async::<u64, 1>(0);
///
/// let handle = thread::spawn(move || {
///     block_on(async {
///         let mut seen = Vec::new();
///         // resolves on each value not yet read, the initial one included, and errors once the writer drops
///         while reader.changed().await.is_ok() {
///             let latest = *reader.latest();
///             seen.push(latest);
///         }
///         seen
///     })
/// });
///
/// for v in 1..=3 {
///     writer.publish(v)?;
/// }
/// drop(writer);
///
/// // intermediate values may be skipped, but the final one is always seen
/// let seen = handle.join().unwrap();
/// let last = seen.last();
/// assert_eq!(last, Some(&3));
/// assert!(seen.is_sorted());
/// # Ok::<(), SwmrError>(())
/// ```
#[cfg(feature = "async")]
pub fn register_async<T: Clone, const N: usize>(
    initial: T,
) -> (AsyncWriter<T>, [AsyncReader<T>; N]) {
    register_with(|| initial.clone())
}

/// Creates a register with `N` readers waiting via `W`, every slot initialized by `init`
///
/// `N` must be in `1..=62`, otherwise compilation fails:
/// ```compile_fail,E0080
/// let _ = veloce::swmr::register::<u64, 0>(0);
/// ```
/// ```compile_fail,E0080
/// let _ = veloce::swmr::register::<u64, 63>(0);
/// ```
pub fn register_with<T, W, const N: usize>(
    init: impl FnMut() -> T,
) -> (Writer<T, W>, [Reader<T, W>; N])
where
    W: WaitStrategy,
{
    const { assert!(N > 0, "N == 0") };
    const { assert!(N <= MAX_READERS, "too many readers: N > MAX_READERS") };

    let register = Register::new(N, init);
    register.split()
}

#[derive(Debug, PartialEq, Eq)]
pub enum SwmrError {
    Disconnected,
    ValueNotStaged,
}
