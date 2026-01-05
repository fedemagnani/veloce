use std::{
    cell::Cell,
    sync::{Arc, atomic::Ordering},
};

use crate::{
    ring::Storable,
    spsc::{Channel, TryRecvError},
};

#[cfg(feature = "async")]
pub use r#async::RecvFuture;
use crossbeam_utils::CachePadded;

pub struct Receiver<T, const N: usize> {
    pub(super) inner: CachePadded<Arc<Channel<T, N>>>,
    /// Local head cursor - only modified by this receiver.
    head: Cell<usize>,
}

impl<T, const N: usize> Receiver<T, N> {
    pub(super) fn new(inner: Arc<Channel<T, N>>) -> Self {
        Self {
            inner: CachePadded::new(inner),
            head: Cell::new(0),
        }
    }

    /// Consumer consumes a value from the buffer using per-slot stamps (Vyukov algorithm).
    ///
    /// Protocol:
    /// - Check slot stamp: if stamp == head + 1, data is ready
    /// - Read value, then set stamp = head + N (signals "slot ready for next write lap")
    /// - Advance local head cursor
    pub fn try_recv(&self) -> Result<Option<T>, TryRecvError> {
        let head = self.head.get();
        let index = self.inner.buffer.index(head);
        let slot = self.inner.buffer.get(index);

        // Acquire: synchronize with sender's Release store after writing
        let stamp = slot.load_stamp();

        if stamp == head.wrapping_add(1) {
            // Data is ready
            let value = unsafe { slot.read() };

            // Release: make the read visible before signaling "slot ready"
            slot.store_stamp(head.wrapping_add(N));

            // Advance local head (Relaxed: we're the only writer)
            self.head.set(head.wrapping_add(1));

            return Ok(Some(value));
        }

        // Buffer is empty: stamp == head means no data written yet
        // Check disconnection only when empty
        if self.is_closed() {
            return Err(TryRecvError);
        }
        Ok(None)
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

    /// Returns true if the channel appears empty.
    ///
    /// This checks the slot at the current head position. For most use cases,
    /// prefer checking if `try_recv()` returns `None`.
    pub fn is_empty(&self) -> bool {
        let head = self.head.get();
        let index = self.inner.buffer.index(head);
        let slot = self.inner.buffer.get(index);
        let stamp = slot.load_stamp();
        // Empty if stamp == head (no data yet)
        stamp == head
    }

    /// Returns approximate number of items in the channel.
    ///
    /// Scans slots starting from head to count consecutive items with data.
    /// This is O(min(count, N)) but typically fast for small queues.
    pub fn len(&self) -> usize {
        let head = self.head.get();
        let mut count = 0;

        while count < N {
            let seq = head.wrapping_add(count);
            let index = self.inner.buffer.index(seq);
            let slot = self.inner.buffer.get(index);
            let stamp = slot.load_stamp();

            // Check if slot has data: stamp == seq + 1
            if stamp == seq.wrapping_add(1) {
                count += 1;
            } else {
                break;
            }
        }

        count
    }

    /// Drains up to `max` available items from the channel.
    ///
    /// Returns an iterator that yields items. The `&mut self` borrow prevents
    /// concurrent access to the receiver until the `Drain` is dropped.
    ///
    /// # Performance
    ///
    /// With per-slot stamps, each item read updates its slot stamp individually,
    /// allowing the producer to see freed slots immediately.
    ///
    /// # Behavior
    ///
    /// - Yields items as long as slots have data (lazy evaluation)
    /// - Does not signal disconnection — check [`is_closed()`](Self::is_closed) after
    /// - Panic-safe: consumed items are committed even if iteration panics
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     for msg in rx.drain(256) {
    ///         process(msg);
    ///     }
    ///     if rx.is_closed() {
    ///         break;
    ///     }
    ///     std::hint::spin_loop();
    /// }
    /// ```
    #[inline]
    pub fn drain(&mut self, max: usize) -> Drain<'_, T, N> {
        Drain {
            rx: self,
            remaining: max,
        }
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

/// Draining iterator created by [`Receiver::drain()`].
///
/// Reads items using per-slot stamps for synchronization.
/// Each item consumed immediately frees its slot for the producer.
pub struct Drain<'a, T, const N: usize> {
    rx: &'a mut Receiver<T, N>,
    /// Maximum items remaining to drain.
    remaining: usize,
}

impl<T, const N: usize> Drain<'_, T, N> {
    /// Returns `true` if the sender has dropped.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.rx.is_closed()
    }

    /// Returns how many items we're still allowed to drain (upper bound).
    #[inline]
    pub fn remaining(&self) -> usize {
        self.remaining
    }
}

impl<T, const N: usize> Iterator for Drain<'_, T, N> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let head = self.rx.head.get();
        let index = self.rx.inner.buffer.index(head);
        let slot = self.rx.inner.buffer.get(index);

        // Acquire: synchronize with sender's Release store
        let stamp = slot.load_stamp();

        if stamp == head.wrapping_add(1) {
            // Data is ready
            let value = unsafe { slot.read() };

            // Release: signal slot is ready for next write lap
            slot.store_stamp(head.wrapping_add(N));

            // Advance head cursor
            self.rx.head.set(head.wrapping_add(1));
            self.remaining -= 1;

            Some(value)
        } else {
            // No more data available
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // Lower bound is 0 (might be empty), upper bound is remaining
        (0, Some(self.remaining))
    }
}

#[cfg(feature = "async")]
mod r#async {

    use std::{
        future::Future,
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
                    // Consume a value from the buffer, waking sender who might be waiting
                    self.wake_sender();
                    Poll::Ready(Ok(v))
                }
                Ok(None) => {
                    // Register waker for future polls
                    self.register_waker(cx.waker());

                    // Double-check: see if data became available
                    let head = self.receiver.head.get();
                    let index = self.receiver.inner.buffer.index(head);
                    let slot = self.receiver.inner.buffer.get(index);
                    let stamp = slot.load_stamp();

                    if stamp == head.wrapping_add(1) {
                        // Data is now available, self-wake
                        cx.waker().wake_by_ref();
                    }

                    Poll::Pending
                }
                Err(e) => Poll::Ready(Err(e)),
            }
        }
    }
}
