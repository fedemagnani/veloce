use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crossbeam_utils::CachePadded;

use crate::{
    ring::RingBuffer,
    spsc::{receiver::Receiver, sender::Sender, slot::Slot},
};

#[cfg(feature = "async")]
use r#async::Wakers;
#[cfg(feature = "async")]
use std::task::Waker;

pub(super) struct Channel<T, const N: usize> {
    pub(super) buffer: RingBuffer<Slot<T>, N>,
    /// Producer's cursor - only modified by sender, can be read by receiver
    pub(super) head: CachePadded<AtomicUsize>,
    /// Consumer's cursor - only modified by receiver, can be read by sender
    pub(super) tail: CachePadded<AtomicUsize>,
    pub(super) closed: CachePadded<AtomicBool>,

    #[cfg(feature = "async")]
    wakers: Wakers,
}

impl<T, const N: usize> Default for Channel<T, N> {
    fn default() -> Self {
        // RingBuffer::new() initializes stamps to [0, 1, 2, ..., N-1]
        let buffer = RingBuffer::default();
        let closed = CachePadded::new(AtomicBool::new(false));
        let head = CachePadded::new(AtomicUsize::new(0));
        let tail = CachePadded::new(AtomicUsize::new(0));
        #[cfg(feature = "async")]
        let wakers = Wakers::default();
        Self {
            buffer,
            closed,
            head,
            tail,
            #[cfg(feature = "async")]
            wakers,
        }
    }
}

impl<T, const N: usize> Channel<T, N> {
    pub fn split(self) -> (Sender<T, N>, Receiver<T, N>) {
        let inner = Arc::new(self);
        let tx = Sender::new(inner.clone());
        let rx = Receiver::new(inner);
        (tx, rx)
    }

    #[inline]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    #[cfg(feature = "async")]
    pub(super) fn wake_sender(&self) {
        self.wakers.wake_sender()
    }

    #[cfg(feature = "async")]
    pub(super) fn wake_receiver(&self) {
        self.wakers.wake_receiver()
    }

    #[cfg(feature = "async")]
    pub(super) fn register_sender_waker(&self, waker: &Waker) {
        self.wakers.register_sender_waker(waker);
    }

    #[cfg(feature = "async")]
    pub(super) fn register_receiver_waker(&self, waker: &Waker) {
        self.wakers.register_receiver_waker(waker);
    }
}

unsafe impl<T: Send, const N: usize> Sync for Channel<T, N> {}
unsafe impl<T: Send, const N: usize> Send for Channel<T, N> {}

// The channel is dropped when both Sender and Receiver have dropped
impl<T, const N: usize> Drop for Channel<T, N> {
    fn drop(&mut self) {
        // Safe using `get_mut` because:
        // 1. Arc's acquire fence synchronized with all Release stores
        // 2. &mut self guarantees exclusive access
        // 3. No atomic operation needed - just reading memory we own exclusively
        let head = *self.head.get_mut();
        let tail = *self.tail.get_mut();
        let count = tail.wrapping_sub(head);
        for s in 0..count {
            let i = self.buffer.index(head.wrapping_add(s));
            // Safe: these slots are initialized (producer wrote, consumer didn't read)
            unsafe { self.buffer.drop_in_place(i) };
        }
    }
}

#[cfg(feature = "async")]
mod r#async {
    use super::*;

    use futures::task::AtomicWaker;
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
