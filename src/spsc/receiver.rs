use std::{
    cell::Cell,
    marker::PhantomData,
    sync::{Arc, atomic::Ordering},
};

use crate::spsc::{Channel, TryRecvError};

#[cfg(feature = "async")]
pub use r#async::RecvFuture;
pub struct Receiver<T, const N: usize> {
    pub(super) inner: Arc<Channel<T, N>>,
    _not_clone: PhantomData<Cell<()>>, //marker type to avoid cloning implementations
}

impl<T, const N: usize> Receiver<T, N> {
    pub(super) fn new(inner: Arc<Channel<T, N>>) -> Self {
        Self {
            inner,
            _not_clone: PhantomData,
        }
    }

    /// Consumer consumes a value from the buffer if it's ready
    pub fn try_recv(&self) -> Result<Option<T>, TryRecvError> {
        // Single consumer: the only one controlling the head
        let head = self.inner.head.load(Ordering::Relaxed);

        // acquire-load: acquire ownership of the tail and observe all writes performed by the previous owner (producer) via release-store
        let tail = self.inner.tail.load(Ordering::Acquire);

        if head == tail {
            // Disconnection check happens only when we are sure that there are no more messages to read
            if self.is_closed() {
                return Err(TryRecvError);
            }

            return Ok(None);
        }

        let i = self.inner.buffer.index(head);

        let out = unsafe { self.inner.buffer.read(i) };

        // release-store: make sure that acquire-loads see also the previous readings on the buffer
        self.inner.head.store(head + 1, Ordering::Release);

        Ok(Some(out))
    }

    /// Receiver retrieves a new value from the buffer using a busy-spin strategy.
    ///
    /// If new value is not ready, it hints to the CPU that it is in a spin-wait
    /// (`hint::spin_loop`), allowing the processor to apply spin-wait
    /// optimizations (e.g. reduced power and SMT contention).
    ///
    /// This favors minimal latency over fairness, and avoids `thread::yield_now`,
    /// which may enter the scheduler and potentially deschedule the thread.
    pub fn recv_spin(&self) -> Result<T, TryRecvError> {
        loop {
            match self.try_recv() {
                Ok(Some(v)) => return Ok(v),
                Err(e) => return Err(e),
                Ok(None) => {
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// Receiver retrieves a new value from the buffer using a async strategy.
    ///
    /// - On success: wakes the sender (if blocked on a full buffer) to signal
    ///   that a slot has been freed.
    /// - On empty buffer: registers a waker and returns `Pending`. A double-check
    ///   is performed after registration to avoid missed wakeups if the sender
    ///   pushed a value in the meantime.
    ///
    /// # Cancel Safety
    ///
    /// This future is cancel-safe. Dropping it before completion does not lose data.
    #[cfg(feature = "async")]
    pub fn recv(&self) -> RecvFuture<'_, T, N> {
        RecvFuture::new(self)
    }

    /// Returns the channel capacity.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Returns true if the sender has been dropped.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Returns true if the channel is empty.
    pub fn is_empty(&self) -> bool {
        let head = self.inner.head.load(Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Acquire);
        head == tail
    }

    /// Returns approximate number of items in the channel.
    pub fn len(&self) -> usize {
        let head = self.inner.head.load(Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }
}

impl<T, const N: usize> Drop for Receiver<T, N> {
    fn drop(&mut self) {
        self.inner.closed.store(true, Ordering::Release);

        #[cfg(feature = "async")]
        // wake the other half to let it acknowledge disconnection
        self.inner.wake_sender();
    }
}

unsafe impl<T: Send, const N: usize> Sync for Receiver<T, N> {}
unsafe impl<T: Send, const N: usize> Send for Receiver<T, N> {}

#[cfg(feature = "async")]
mod r#async {

    use std::{
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    use super::*;

    #[must_use = "futures do nothing unless polled"]
    pub struct RecvFuture<'a, T, const N: usize> {
        receiver: &'a Receiver<T, N>,
    }

    /// Safe: the struct is not self-referential:
    /// future fields are not pointing to other fields within the same struct
    impl<T, const N: usize> Unpin for RecvFuture<'_, T, N> {}

    impl<'a, T, const N: usize> RecvFuture<'a, T, N> {
        pub fn new(receiver: &'a Receiver<T, N>) -> Self {
            Self { receiver }
        }

        fn register_waker(&self, waker: &Waker) {
            self.receiver.inner.register_receiver_waker(waker);
        }

        fn wake_sender(&self) {
            self.receiver.inner.wake_sender();
        }
    }

    impl<'a, T, const N: usize> Future for RecvFuture<'a, T, N> {
        type Output = Result<T, TryRecvError>;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            match self.receiver.try_recv() {
                Ok(Some(v)) => {
                    // Consume a value from the buffer, waking sender who might be waiting for some free space in the buffer
                    self.wake_sender();
                    Poll::Ready(Ok(v))
                }
                Ok(None) => {
                    // we store the waker for future polls
                    self.register_waker(cx.waker());

                    // We give a second shot to see if we should be woken up immediately
                    let tail = self.receiver.inner.tail.load(Ordering::Acquire);
                    let head = self.receiver.inner.head.load(Ordering::Relaxed);

                    // Check if producer pushed some data in the meanwhile
                    if tail != head {
                        // New data is available, self-wake to try another recv attempt (via the waker just registered)
                        cx.waker().wake_by_ref();
                    }
                    Poll::Pending
                }
                Err(e) => Poll::Ready(Err(e)),
            }
        }
    }
}
