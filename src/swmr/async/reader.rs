use std::{
    future::Future,
    pin::Pin,
    sync::atomic::Ordering,
    task::{Context, Poll},
};

use super::AsyncReader;
use crate::swmr::SwmrError;

impl<T> AsyncReader<T> {
    /// Waits for a value not yet returned by [`Reader::latest`](crate::swmr::Reader::latest), erroring once the `Writer` drops
    pub fn changed(&mut self) -> Changed<'_, T> {
        Changed { reader: self }
    }

    /// `Some` if [`Changed`] can resolve without waiting
    fn poll_changed(&self) -> Option<Result<(), SwmrError>> {
        // SeqCst pairs with the writer advancing `current` before notifying
        let current = self.register.current.load(Ordering::SeqCst);
        let current_version = current.version();
        if self.version() != Some(current_version) {
            return Some(Ok(()));
        }
        if current.is_closed() {
            return Some(Err(SwmrError::Disconnected));
        }
        None
    }
}

/// Future returned by [`Reader::changed`](AsyncReader::changed)
#[must_use = "futures do nothing unless polled"]
pub struct Changed<'a, T> {
    reader: &'a mut AsyncReader<T>,
}

impl<T> Future for Changed<'_, T> {
    type Output = Result<(), SwmrError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let reader = &*self.reader;
        if let Some(ready) = reader.poll_changed() {
            return Poll::Ready(ready);
        }

        let strategy = reader.strategy();
        strategy.waker.register(cx.waker());
        // SeqCst pairs with the writer's notify: the re-check sees the commit, or it sees this flag
        strategy.waiting.store(true, Ordering::SeqCst);

        // re-check: the writer may have committed before observing the flag
        if let Some(ready) = reader.poll_changed() {
            return Poll::Ready(ready);
        }

        Poll::Pending
    }
}

impl<T> Drop for Changed<'_, T> {
    /// Stops the writer from waking this reader: a stale flag only causes a spurious wake-up
    fn drop(&mut self) {
        let strategy = self.reader.strategy();
        strategy.waiting.store(false, Ordering::Relaxed);
    }
}
