use crate::spsc::{Channel, TrySendErr};
use std::{
    cell::Cell,
    marker::PhantomData,
    sync::{Arc, atomic::Ordering},
};

#[cfg(feature = "async")]
pub use r#async::SendFuture;

pub struct Sender<T, const N: usize> {
    pub(super) inner: Arc<Channel<T, N>>,
    _not_clone: PhantomData<Cell<()>>, //marker type to avoid cloning implementations
}

impl<T, const N: usize> Sender<T, N> {
    pub(super) fn new(inner: Arc<Channel<T, N>>) -> Self {
        Self {
            inner,
            _not_clone: PhantomData,
        }
    }

    /// Producer pushes a new value in the buffer
    pub fn try_send(&self, value: T) -> Result<(), TrySendErr<T>> {
        if self.is_closed() {
            return Err(TrySendErr::Disconnected(value));
        }

        // Single producer: the only one controlling the tail
        let tail = self.inner.tail.load(Ordering::Relaxed);

        // acquire-load: acquire ownership of the head and observe all writes performed by the previous owner (consumer) via release-store
        let head = self.inner.head.load(Ordering::Acquire);

        if tail.wrapping_sub(head) >= N {
            // slow consumer
            return Err(TrySendErr::Full(value));
        }

        let i = self.inner.buffer.index(tail);

        // # Safety
        //
        // This assumes that every read *moves* the value out of the slot.
        // Therefore, the value must NOT be dropped in place.
        // If a read only borrows the value and the producer later overwrites this slot,
        // the destructor of the previous value would never run, resulting in a memory leak.

        unsafe { self.inner.buffer.write(i, value) };

        // release-store: make sure that acquire-loads see also the previous writings on the buffer
        self.inner.tail.store(tail + 1, Ordering::Release);

        Ok(())
    }

    /// Producer pushes a new value into the buffer using a busy-spin strategy.
    ///
    /// If the channel is full, it hints to the CPU that it is in a spin-wait
    /// (`hint::spin_loop`), allowing the processor to apply spin-wait
    /// optimizations (e.g. reduced power and SMT contention).
    ///
    /// This favors minimal latency over fairness, and avoids `thread::yield_now`,
    /// which may enter the scheduler and potentially deschedule the thread.
    pub fn send_spin(&self, mut value: T) -> Result<(), TrySendErr<T>> {
        loop {
            match self.try_send(value) {
                Ok(()) => return Ok(()),
                Err(TrySendErr::Disconnected(v)) => return Err(TrySendErr::Disconnected(v)),
                Err(TrySendErr::Full(v)) => {
                    value = v;
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// Producer pushes a new value into the buffer using a async strategy.
    ///
    /// - If a new value is successfully pushed, the receiver's waker
    ///   is notified so a blocked [`RecvFuture`] can proceed.
    /// - if  the buffer is full, the sender's waker is registered and
    ///   a double-check is performed: if space became available concurrently, the
    ///   future self-wakes to avoid a missed wakeup.
    ///
    /// # Cancel Safety
    ///
    /// **Not cancel-safe.** Dropping this future before completion loses the value.
    #[cfg(feature = "async")]
    pub fn send(&self, value: T) -> SendFuture<'_, T, N> {
        SendFuture::new(self, value)
    }

    /// Returns the channel capacity.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Returns true if the receiver has been dropped.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

impl<T, const N: usize> Drop for Sender<T, N> {
    fn drop(&mut self) {
        self.inner.closed.store(true, Ordering::Release);

        // wake the other half to let it acknowledge disconnection
        #[cfg(feature = "async")]
        self.inner.wake_receiver();
    }
}

unsafe impl<T: Send, const N: usize> Sync for Sender<T, N> {}
unsafe impl<T: Send, const N: usize> Send for Sender<T, N> {}

#[cfg(feature = "async")]
mod r#async {
    use std::{
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    use super::*;

    #[must_use = "futures do nothing unless polled"]
    pub struct SendFuture<'a, T, const N: usize> {
        sender: &'a Sender<T, N>,
        value: Option<T>,
    }

    /// Safe: the struct is not self-referential:
    /// future fields are not pointing to other fields within the same struct
    impl<T, const N: usize> Unpin for SendFuture<'_, T, N> {}

    impl<'a, T, const N: usize> SendFuture<'a, T, N> {
        pub fn new(sender: &'a Sender<T, N>, value: T) -> Self {
            Self {
                sender,
                value: Some(value),
            }
        }

        fn register_waker(&self, waker: &Waker) {
            self.sender.inner.register_sender_waker(waker);
        }

        fn wake_receiver(&self) {
            self.sender.inner.wake_receiver();
        }
    }

    impl<'a, T, const N: usize> Future for SendFuture<'a, T, N> {
        type Output = Result<(), TrySendErr<T>>;
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let value = self.value.take().expect("polled after completion");

            match self.sender.try_send(value) {
                Ok(()) => {
                    // Notify the receiver of the new value
                    self.wake_receiver();
                    Poll::Ready(Ok(()))
                }
                Err(TrySendErr::Disconnected(v)) => {
                    // No need to notify as the other half is probably dropped
                    Poll::Ready(Err(TrySendErr::Disconnected(v)))
                }

                Err(TrySendErr::Full(v)) => {
                    // we put back the value for future polls
                    self.value = Some(v);

                    // we store the waker for future polls
                    self.register_waker(cx.waker());

                    // We give a second shot to see if we should be woken up immediately
                    let head = self.sender.inner.head.load(Ordering::Acquire);
                    let tail = self.sender.inner.tail.load(Ordering::Relaxed);

                    // Check if consumer freed some space in the meanwhile
                    if tail.wrapping_sub(head) < N {
                        // Channel is not full anymore, self-wake to try another send attempt (via the waker just registered)
                        cx.waker().wake_by_ref();
                    }

                    Poll::Pending
                }
            }
        }
    }
}
