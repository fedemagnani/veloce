//! Single-Producer Single-Consumer (SPSC) Channels
//!
//! Lock-free bounded channels built on ring buffers for fast, allocation-free
//! message passing between exactly one producer and one consumer thread.
//!
//! ## Implementations
//!
//! This module provides two alternative SPSC algorithms:
//!
//! - [`lamport`] — Classic approach with shared atomic head/tail indices
//! - [`vyukov`] — Per-slot sequence stamps for reduced cache contention

pub mod lamport;
pub mod vyukov;

#[derive(Debug)]
pub enum TrySendErr<T> {
    Full(T),
    Disconnected(T),
}

/// Thrown on disconnected channel
#[derive(Debug)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

#[cfg(feature = "async")]
mod r#async {

    use crossbeam_utils::CachePadded;
    use futures::task::AtomicWaker;
    use std::task::Waker;

    pub(super) struct Wakers {
        pub(super) sender_waker: CachePadded<AtomicWaker>,
        pub(super) receiver_waker: CachePadded<AtomicWaker>,
    }

    impl Default for Wakers {
        fn default() -> Self {
            Self {
                sender_waker: CachePadded::new(AtomicWaker::new()),
                receiver_waker: CachePadded::new(AtomicWaker::new()),
            }
        }
    }

    impl Wakers {
        pub(super) fn wake_sender(&self) {
            self.sender_waker.wake()
        }

        pub(super) fn wake_receiver(&self) {
            self.receiver_waker.wake()
        }

        pub(super) fn register_sender_waker(&self, waker: &Waker) {
            self.sender_waker.register(waker);
        }

        pub(super) fn register_receiver_waker(&self, waker: &Waker) {
            self.receiver_waker.register(waker);
        }
    }
}
