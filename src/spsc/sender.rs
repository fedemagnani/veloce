use crate::{
    ring::Storable,
    spsc::{Channel, TrySendErr},
};
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

    /// Producer pushes a new value in the buffer using per-slot stamps (Vyukov algorithm).
    ///
    /// Protocol:
    /// - Check slot stamp: if stamp == tail, slot is ready for writing
    /// - Write value, then set stamp = tail + 1 (signals "data ready")
    /// - Advance tail with Relaxed (only sender modifies tail)
    pub fn try_send(&self, value: T) -> Result<(), TrySendErr<T>> {
        if self.is_closed() {
            return Err(TrySendErr::Disconnected(value));
        }

        // Only sender modifies tail, so Relaxed is sufficient
        let tail = self.inner.tail.load(Ordering::Relaxed);
        let index = self.inner.buffer.index(tail);
        let slot = self.inner.buffer.get(index);

        // Acquire: synchronize with receiver's Release store after reading
        let stamp = slot.load_stamp();

        if stamp == tail {
            // Slot is ready for writing
            // Safety: we have exclusive access to this slot (stamp == tail)
            unsafe { slot.write(value) };

            // Release: make the write visible before signaling "data ready"
            slot.store_stamp(tail.wrapping_add(1));

            // Advance tail (Relaxed: only sender reads/writes tail)
            self.inner
                .tail
                .store(tail.wrapping_add(1), Ordering::Relaxed);

            Ok(())
        } else {
            // Buffer is full: stamp should be (tail - N + 1), meaning receiver
            // hasn't consumed this slot from the previous lap yet
            Err(TrySendErr::Full(value))
        }
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

                    // Double-check: see if space became available
                    // With slot stamps, we check the slot at current tail
                    let tail = self.sender.inner.tail.load(Ordering::Relaxed);
                    let index = self.sender.inner.buffer.index(tail);
                    let slot = self.sender.inner.buffer.get(index);
                    let stamp = slot.load_stamp();

                    if stamp == tail {
                        // Slot is now ready, self-wake
                        cx.waker().wake_by_ref();
                    }

                    Poll::Pending
                }
            }
        }
    }
}
