use std::{
    cell::Cell,
    marker::PhantomData,
    sync::{Arc, atomic::Ordering},
};

use super::{Cursors, TryRecvError, channel::Channel};

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
        let cursors = self.cursors();

        if cursors.is_empty() {
            // Disconnection check happens only when we are sure that there are no more messages to read
            if self.is_closed() {
                return Err(TryRecvError);
            }

            return Ok(None);
        }

        let head = cursors.head;

        // Maps the head to the ring-buffer index and read the value
        let out = unsafe { self.read(head) };

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
        self.cursors().is_empty()
    }

    /// Returns approximate number of items in the channel.
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
    /// Synchronization is batched: one `Acquire` load at construction, one `Release`
    /// store on drop. This is faster than calling [`try_recv()`](Self::try_recv) in a
    /// loop when processing multiple items.
    ///
    /// The trade-off: the producer won't see freed slots until the `Drain` drops. This
    /// could cause some backpressure if the producer is particularly spammy and [`Drain`]
    /// lives for too long (for example, if the consumer is slow in processing updates)
    ///
    ///
    /// # Behavior
    ///
    /// - Yields only items available at construction (snapshot semantics)
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
        let mut cursors = self.cursors();
        let original_head = cursors.head;

        // Clamp tail so we yield at most `max` items.
        // Compare counts (not raw sequence numbers) to handle wrap-around.
        let available = cursors.remaining();
        if max < available {
            cursors.tail = original_head.wrapping_add(max);
        }

        Drain {
            rx: self,
            original_head,
            cursors,
        }
    }

    /// Returns the `head` and `tail` of the channel.
    ///
    /// The `tail` is retrieved first via relaxed load to early exit if there is no new data,
    /// then via [`Ordering::Acquire`]: it shouldn't be called more than once
    fn cursors(&self) -> Cursors {
        // Single consumer: the only one controlling the head
        let head = self.inner.head.load(Ordering::Relaxed);

        // acquire-load: acquire ownership of the tail and observe all writes
        // performed by the previous owner (producer) via release-store
        let tail = self.inner.tail.load(Ordering::Acquire);

        Cursors { head, tail }
    }

    /// Maps the sequence to the ring-buffer index, reading the value from the buffer.
    ///
    /// Notice: it doesn't update the head
    unsafe fn read(&self, seq: usize) -> T {
        let i = self.inner.buffer.index(seq);
        unsafe { self.inner.buffer.read(i) }
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
/// Reads items from `[original_head, tail)` without per-item synchronization.
/// On drop, commits all consumed items with a single `Release` store.
pub struct Drain<'a, T, const N: usize> {
    rx: &'a mut Receiver<T, N>,
    /// Head at construction; used to detect if anything was consumed.
    original_head: usize,
    /// `head` advances during iteration; `tail` is fixed at construction.
    cursors: Cursors,
}

impl<T, const N: usize> Drain<'_, T, N> {
    /// Writes the current head back to the channel (Release).
    /// Skipped if nothing was consumed.
    #[inline]
    fn commit_head(&self) {
        if self.original_head != self.cursors.head {
            self.rx
                .inner
                .head
                .store(self.cursors.head, Ordering::Release);
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
        self.cursors.remaining()
    }
}

impl<T, const N: usize> Iterator for Drain<'_, T, N> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.cursors.is_empty() {
            return None;
        }

        let head = self.cursors.head;

        // Safety: head < tail, so slot is initialized. The Acquire on tail
        // at construction synchronized with the producer's Release store.
        let out = unsafe { self.rx.read(head) };

        // Update ephemeral head (real head is updated on `drop`)
        self.cursors.head += 1;
        Some(out)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let r = self.cursors.remaining();
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
