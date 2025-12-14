//! Lock-free SPSC Channel
//!
//! A bounded, single-producer single-consumer (SPSC) channel implemented using
//! a lock-free ring buffer with atomic head/tail indices.
//!
//! ## How It Works
//!
//!```
//!                   tail (producer writes here)
//!                   ↓
//! ┌───┬───┬───┬───┬───┬───┬───┬───┐
//! │ 0 │ 1 │ 2 │ 3 │ 4 │ 5 │ 6 │ 7 │  Ring Buffer (N = 8)
//! └───┴───┴───┴───┴───┴───┴───┴───┘
//!       ↑
//!       head (consumer reads here)
//!```
//!
//! - **Producer** writes to `buffer[tail % N]`, then increments `tail`
//! - **Consumer** reads from `buffer[head % N]`, then increments `head`
//! - **Buffer full**: `tail - head >= N`
//! - **Buffer empty**: `tail == head`
//!
//! ## Synchronization
//!
//! No locks or OS primitives are used. Synchronization relies on:
//!
//! | Operation | Memory Ordering | Purpose |
//! |-----------|-----------------|---------|
//! | Read own index | `Relaxed` | Only one thread modifies it |
//! | Read other's index | `Acquire` | See their writes to the buffer |
//! | Write own index | `Release` | Make buffer writes visible |
//!
//! The `Acquire`/`Release` pairing ensures that
//! - when the consumer sees a new `tail` value, it also sees the data the producer wrote to the buffer.
//! - when the producer sees a new `head` value, it also sees that the consumer has read data from the buffer.
//!
//! ## Cache Optimization
//!
//! Most of the fields of [`Channel`] are cache-padded ([`CachePadded`](crossbeam_utils::CachePadded))
//! to prevent false sharing between producer and consumer threads.
//!
//! ## Async Support
//!
//! With the `async` feature, [`send()`](Sender::send) and [`recv()`](Receiver::recv)
//! return futures that poll the underlying lock-free operations. The futures
//! themselves make no OS calls—whether the OS is involved depends on your runtime
//!
//! ## Example
//!
//!```
//! use veloce::spsc::channel;
//!
//! let (tx, rx) = channel4::<i32>();  // Buffer size must be power of 2
//!
//! tx.try_send(1).unwrap();
//! tx.try_send(2).unwrap();
//!
//! assert_eq!(rx.try_recv().unwrap(), Some(1));
//! assert_eq!(rx.try_recv().unwrap(), Some(2));
//! assert_eq!(rx.try_recv().unwrap(), None);  // Empty
//! ```
mod channel;
mod error;
mod receiver;
mod sender;

use channel::Channel;
pub use error::*;
pub use receiver::Receiver;
#[cfg(feature = "async")]
pub use receiver::RecvFuture;
#[cfg(feature = "async")]
pub use sender::SendFuture;
pub use sender::Sender;
pub fn channel<T, const N: usize>() -> (Sender<T, N>, Receiver<T, N>) {
    Channel::default().split()
}

/// Generates type aliases for common buffer sizes.
///
/// Creates types like `Sender2<T>`, `channel16<T>`, `Receiver64<T>`, etc.
macro_rules! define_size_aliases {
    ($($n:literal),* $(,)?) => {
        paste::paste! {
            $(
                pub type [<Sender $n>]<T> = Sender<T, $n>;
                pub type [<Receiver $n>]<T> = Receiver<T, $n>;

                #[cfg(feature = "async")]
                pub type [<SendFuture $n>]<'a, T> = SendFuture<'a, T, $n>;
                #[cfg(feature = "async")]
                pub type [<RecvFuture $n>]<'a, T> = RecvFuture<'a, T, $n>;

                /// Creates a channel with buffer size $n.
                pub fn [<channel $n>]<T>() -> ([<Sender $n>]<T>, [<Receiver $n>]<T>) {
                    channel::<T, $n>()
                }
            )*
        }
    };
}

// Generate aliases for powers of 2
define_size_aliases!(2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192);

#[cfg(test)]
mod tests {

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::{sync::Arc, thread::sleep, time::Duration};

    use super::*;

    /// When buffer is full, sender shouldn't be capable to push a new value
    #[test]
    fn test_full() {
        const N: usize = 4;
        let (tx, _rx) = channel::<(), N>();
        for _ in 0..N {
            tx.try_send(()).unwrap();
        }
        let err = tx.try_send(()).expect_err("should err");
        assert!(matches!(err, TrySendErr::Full(..)))
    }

    /// When one of the two half drops, the channels should me marked as disconnected
    #[test]
    fn test_disconnected() {
        let (tx, rx) = channel::<(), 16>();
        assert!(!tx.is_closed());
        assert!(!rx.is_closed());

        let (tx, ..) = channel::<(), 16>();
        assert!(tx.is_closed());

        let (.., rx) = channel::<(), 16>();
        assert!(rx.is_closed());
    }

    /// The consumer should be capable to read all the buffered messages, even if producer dropped
    #[test]
    fn test_proper_consumption() {
        const N: usize = 4;
        let (tx, rx) = channel::<(), N>();
        for _ in 0..N {
            tx.try_send(()).unwrap();
        }

        drop(tx);

        for _ in 0..N {
            rx.try_recv().unwrap();
        }

        rx.try_recv().expect_err("should err");
    }

    /// Inter-thread communication check
    #[test]
    fn test_channel() {
        let (tx, rx) = channel::<_, 2>();

        let words = [
            String::from("hello"),
            String::from("world"),
            String::from("!"),
        ];

        let words_c = words.clone();
        std::thread::spawn(move || {
            for w in words_c {
                tx.try_send(w).unwrap();
                sleep(Duration::from_nanos(1));
            }
        });

        for w in words {
            'i: loop {
                if let Ok(Some(out)) = rx.try_recv() {
                    assert_eq!(out, w);
                    break 'i;
                }
            }
        }
    }

    #[derive(Debug, Clone)]
    struct DropCounter(Arc<AtomicUsize>);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    // Make sure that, when channel is dropped, the buffered elements are dropped as well (no memory leak)
    #[test]
    fn test_drop_unread_items() {
        let inner: AtomicUsize = AtomicUsize::new(0);
        let inner = Arc::new(inner);
        let dropper = DropCounter(inner.clone());

        {
            let (tx, rx) = channel::<DropCounter, 4>();
            tx.try_send(dropper.clone()).unwrap();
            tx.try_send(dropper).unwrap();
            drop(rx);
            drop(tx);
        }
        assert_eq!(inner.load(Ordering::SeqCst), 2);
    }

    /// Test the async strategy
    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_async_channel() {
        let (tx, rx) = channel::<i32, 8>();

        let handle = tokio::spawn(async move {
            for i in 0..10 {
                tx.send(i).await.unwrap();
            }
        });

        for i in 0..10 {
            assert_eq!(rx.recv().await.unwrap(), i);
        }

        handle.await.unwrap();
    }
}
