use std::{
    cell::Cell,
    marker::PhantomData,
    sync::{Arc, atomic::Ordering},
};

use crate::{
    ring::Storable,
    spsc::{Channel, Cursors, TryRecvError},
};

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

    /// Consumer consumes a value from the buffer using per-slot stamps (Vyukov algorithm).
    ///
    /// Protocol:
    /// - Check slot stamp: if stamp == head + 1, data is ready
    /// - Read value, then set stamp = head + N (signals "slot ready for next write lap")
    /// - Advance head with Relaxed (only receiver modifies head)
    pub fn try_recv(&self) -> Result<Option<T>, TryRecvError> {
        // Only receiver modifies head, so Relaxed is sufficient
        let head = self.inner.head.load(Ordering::Relaxed);
        let index = self.inner.buffer.index(head);
        let slot = self.inner.buffer.get(index);

        // Acquire: synchronize with sender's Release store after writing
        let stamp = slot.load_stamp();

        if stamp == head.wrapping_add(1) {
            // Data is ready
            // Safety: we have exclusive access to read (stamp == head + 1)
            let value = unsafe { slot.read() };

            // Release: make the read visible before signaling "slot ready"
            slot.store_stamp(head.wrapping_add(N));

            // Advance head (Relaxed: only receiver reads/writes head)
            self.inner
                .head
                .store(head.wrapping_add(1), Ordering::Relaxed);

            return Ok(Some(value));
        }
        // If we are here, buffer is empty: stamp == head means no data written yet
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

    /// Returns true if the channel is empty.
    ///
    /// Note: This uses the legacy cursor-based check for compatibility.
    /// For most use cases, prefer checking if try_recv returns None.
    pub fn is_empty(&self) -> bool {
        self.cursors().is_empty()
    }

    /// Returns approximate number of items in the channel.
    ///
    /// Note: This is approximate as it reads both cursors non-atomically.
    pub fn len(&self) -> usize {
        self.cursors().remaining()
    }

    /// Drains up to `max` available items from the channel.
    ///
    /// Returns an iterator that yields `min(max, available)` items. The `&mut self`
    /// borrow prevents concurrent access to the receiver until the `Drain` is dropped.
    ///
    /// # Performance
    ///
    /// With per-slot stamps, each item read updates its slot stamp individually.
    /// However, the head cursor is only updated once on drop, batching that update.
    ///
    /// The trade-off: the producer won't see freed slots until the `Drain` drops
    /// AND the slot stamps are updated. With per-slot stamps, the producer can
    /// actually see individual slots become available as they're read.
    ///
    /// # Behavior
    ///
    /// - Yields only items available at construction (snapshot semantics via cursor check)
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
        let cursors = self.cursors();
        let original_head = cursors.head;

        // Clamp tail so we yield at most `max` items.
        // Compare counts (not raw sequence numbers) to handle wrap-around.
        let available = cursors.remaining();
        let limit = if max < available {
            original_head.wrapping_add(max)
        } else {
            cursors.tail
        };

        Drain {
            rx: self,
            original_head,
            current_head: original_head,
            limit,
        }
    }

    /// Returns the `head` and `tail` of the channel.
    ///
    /// Used for len() and is_empty() checks. For actual recv operations,
    /// we use per-slot stamps instead.
    fn cursors(&self) -> Cursors {
        // Both loads are Relaxed since this is just an approximate snapshot
        let head = self.inner.head.load(Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Relaxed);
        Cursors { head, tail }
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
/// On drop, commits the final head position.
pub struct Drain<'a, T, const N: usize> {
    rx: &'a mut Receiver<T, N>,
    /// Head at construction; used to detect if anything was consumed.
    original_head: usize,
    /// Current head position (advances during iteration).
    current_head: usize,
    /// Upper limit (exclusive) for this drain.
    limit: usize,
}

impl<T, const N: usize> Drain<'_, T, N> {
    /// Writes the current head back to the channel.
    /// Skipped if nothing was consumed.
    #[inline]
    fn commit_head(&self) {
        if self.original_head != self.current_head {
            self.rx
                .inner
                .head
                .store(self.current_head, Ordering::Relaxed);
        }
    }

    /// Returns `true` if the sender has dropped.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.rx.is_closed()
    }

    /// Returns how many items are left in this drain.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.limit.wrapping_sub(self.current_head)
    }
}

impl<T, const N: usize> Iterator for Drain<'_, T, N> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_head == self.limit {
            return None;
        }

        let head = self.current_head;
        let index = self.rx.inner.buffer.index(head);
        let slot = self.rx.inner.buffer.get(index);

        // Acquire: synchronize with sender's Release store
        let stamp = slot.load_stamp();

        if stamp == head.wrapping_add(1) {
            // Data is ready
            // Safety: stamp == head + 1 means sender wrote this slot
            let value = unsafe { slot.read() };

            // Release: signal slot is ready for next write lap
            slot.store_stamp(head.wrapping_add(N));

            self.current_head = head.wrapping_add(1);
            Some(value)
        } else {
            // No more data available (shouldn't happen within limit, but be safe)
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let r = self.remaining();
        (r, Some(r))
    }
}

impl<T, const N: usize> ExactSizeIterator for Drain<'_, T, N> {}

impl<T, const N: usize> Drop for Drain<'_, T, N> {
    fn drop(&mut self) {
        self.commit_head();
    }
}

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
                    // Consume a value from the buffer, waking sender who might be waiting
                    self.wake_sender();
                    Poll::Ready(Ok(v))
                }
                Ok(None) => {
                    // Register waker for future polls
                    self.register_waker(cx.waker());

                    // Double-check: see if data became available
                    // With slot stamps, we check the slot at current head
                    let head = self.receiver.inner.head.load(Ordering::Relaxed);
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
