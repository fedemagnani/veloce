use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crossbeam_utils::CachePadded;

use super::{receiver::Receiver, sender::Sender, slot::Slot};
use crate::ring::RingBuffer;

#[cfg(feature = "async")]
use r#async::Wakers;
#[cfg(feature = "async")]
use std::task::Waker;

pub(super) struct Channel<T, const N: usize> {
    pub(super) buffer: RingBuffer<Slot<T>, N>,
    pub(super) closed: CachePadded<AtomicBool>,

    #[cfg(feature = "async")]
    wakers: Wakers,
}

impl<T, const N: usize> Default for Channel<T, N> {
    fn default() -> Self {
        let buffer = RingBuffer::default();
        let closed = CachePadded::new(AtomicBool::new(false));
        #[cfg(feature = "async")]
        let wakers = Wakers::default();
        Self {
            buffer,
            closed,
            #[cfg(feature = "async")]
            wakers,
        }
    }
}

impl<T, const N: usize> Channel<T, N> {
    const MASK: usize = N - 1;

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
        // With per-slot stamps, we determine which slots have unread data by examining stamps.
        //
        // Slot stamp protocol:
        // - Empty (ready for write): stamp % N == index
        // - Has data (ready for read): stamp % N == (index + 1) % N
        //
        // Since N is a power of 2, we use bitwise AND for modulo.
        for i in 0..N {
            let slot = self.buffer.get(i);
            // Relaxed is fine here: we have exclusive access (&mut self) and
            // Arc's drop synchronized with all previous Release stores
            let stamp = slot.stamp.load(Ordering::Relaxed);

            // Check if slot has unread data: stamp % N == (index + 1) % N
            if (stamp & Self::MASK) == ((i + 1) & Self::MASK) {
                // Safe: slot contains initialized data that was never consumed
                unsafe { self.buffer.drop_in_place(i) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Helper to check if a slot "has data" using the stamp check from Drop
    fn slot_has_data<const N: usize>(stamp: usize, index: usize) -> bool {
        (stamp & (N - 1)) == ((index + 1) & (N - 1))
    }

    /// Verify the stamp % N == (index + 1) % N formula for detecting "has data"
    #[test]
    fn test_stamp_has_data_formula() {
        const N: usize = 4;

        // For each slot index, verify the formula across multiple laps
        for index in 0..N {
            // Stamps that indicate EMPTY (ready for write)
            // Pattern: index, index+N, index+2N, ...
            for lap in 0..10 {
                let empty_stamp = index + lap * N;
                assert!(
                    !slot_has_data::<N>(empty_stamp, index),
                    "slot {} with stamp {} should be EMPTY",
                    index,
                    empty_stamp
                );
            }

            // Stamps that indicate HAS DATA (ready for read)
            // Pattern: index+1, index+1+N, index+1+2N, ...
            for lap in 0..10 {
                let data_stamp = index + 1 + lap * N;
                assert!(
                    slot_has_data::<N>(data_stamp, index),
                    "slot {} with stamp {} should HAVE DATA",
                    index,
                    data_stamp
                );
            }
        }
    }

    /// Edge case: slot at index N-1 (last slot)
    /// After write: stamp = N, which % N = 0 = (N-1+1) % N ✓
    #[test]
    fn test_stamp_last_slot_edge_case() {
        const N: usize = 4;
        let index = N - 1; // index = 3

        // Initial: stamp = 3 (empty)
        assert!(!slot_has_data::<N>(3, index));

        // After write at tail=3: stamp = 4
        // Check: 4 % 4 = 0, (3+1) % 4 = 0 ✓
        assert!(slot_has_data::<N>(4, index));

        // After read at head=3: stamp = 7
        // Check: 7 % 4 = 3, (3+1) % 4 = 0 ✗ (correctly detected as empty)
        assert!(!slot_has_data::<N>(7, index));

        // After write at tail=7: stamp = 8
        // Check: 8 % 4 = 0, (3+1) % 4 = 0 ✓
        assert!(slot_has_data::<N>(8, index));
    }

    /// Test that Channel::drop correctly drops unread items using stamp detection
    #[test]
    fn test_channel_drop_partial_consumption() {
        let drop_count = Arc::new(AtomicUsize::new(0));

        #[derive(Clone, Debug)]
        struct DropCounter(Arc<AtomicUsize>);
        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }

        // Scenario: write 3 items, read 1, drop channel → should drop 2 remaining
        {
            let (tx, rx) = crate::spsc::vyukov::channel::<DropCounter, 4>();

            tx.try_send(DropCounter(drop_count.clone())).unwrap();
            tx.try_send(DropCounter(drop_count.clone())).unwrap();
            tx.try_send(DropCounter(drop_count.clone())).unwrap();

            // Read 1 item (this drops it)
            let _ = rx.try_recv().unwrap();

            // Now drop_count should be 1 (from the read)
            assert_eq!(drop_count.load(std::sync::atomic::Ordering::SeqCst), 1);

            // Drop both sender and receiver → Channel::drop runs
            drop(tx);
            drop(rx);
        }

        // After Channel::drop, 2 more items should be dropped (total: 3)
        assert_eq!(drop_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    /// Test wrap-around: write N items, read all, write more, read some, drop
    #[test]
    fn test_channel_drop_after_wraparound() {
        let drop_count = Arc::new(AtomicUsize::new(0));

        #[derive(Clone, Debug)]
        struct DropCounter(Arc<AtomicUsize>);
        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }

        {
            let (tx, rx) = crate::spsc::vyukov::channel::<DropCounter, 4>();

            // First lap: write 4, read 4
            for _ in 0..4 {
                tx.try_send(DropCounter(drop_count.clone())).unwrap();
            }
            for _ in 0..4 {
                let _ = rx.try_recv().unwrap();
            }

            // At this point: 4 items dropped via read
            assert_eq!(drop_count.load(std::sync::atomic::Ordering::SeqCst), 4);

            // Second lap: write 3, read 1
            for _ in 0..3 {
                tx.try_send(DropCounter(drop_count.clone())).unwrap();
            }
            let _ = rx.try_recv().unwrap(); // Read 1

            // 5 dropped so far (4 from first lap + 1 from second lap read)
            assert_eq!(drop_count.load(std::sync::atomic::Ordering::SeqCst), 5);

            // Drop channel → should drop 2 remaining from second lap
            drop(tx);
            drop(rx);
        }

        // Total: 4 + 1 + 2 = 7
        assert_eq!(drop_count.load(std::sync::atomic::Ordering::SeqCst), 7);
    }

    /// Test empty channel drop (no items to drop)
    #[test]
    fn test_channel_drop_empty() {
        let drop_count = Arc::new(AtomicUsize::new(0));

        #[derive(Clone, Debug)]
        struct DropCounter(Arc<AtomicUsize>);
        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }

        {
            let (tx, rx) = crate::spsc::vyukov::channel::<DropCounter, 4>();
            drop(tx);
            drop(rx);
        }

        // No items were ever written, so nothing to drop
        assert_eq!(drop_count.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    /// Test the specific case where slot 0 and slot 1 both have the same stamp value
    /// but only slot 0 has data. This happens after:
    /// 1. First lap: write all N, read all N
    /// 2. Second lap: write to slot 0 only
    ///
    /// At this point:
    /// - Slot 0: stamp = N+1 (has data from second write)
    /// - Slot 1: stamp = N+1 (from first lap read: 1 + N = N+1)
    ///
    /// The formula must correctly identify only slot 0 as having data.
    #[test]
    fn test_same_stamp_different_slots() {
        const N: usize = 4;

        // After first lap read, slot 1 has stamp = 1 + N = 5
        // After second lap write to slot 0, slot 0 has stamp = N + 1 = 5
        // Both have stamp = 5, but only slot 0 has data!

        let slot0_stamp = N + 1; // 5 - after second write
        let slot1_stamp = 1 + N; // 5 - after first read

        assert_eq!(slot0_stamp, slot1_stamp); // Both are 5!

        // But the formula correctly distinguishes them:
        // Slot 0: (5 & 3) = 1, ((0+1) & 3) = 1 → HAS DATA
        assert!(
            slot_has_data::<N>(slot0_stamp, 0),
            "slot 0 should have data"
        );

        // Slot 1: (5 & 3) = 1, ((1+1) & 3) = 2 → EMPTY
        assert!(
            !slot_has_data::<N>(slot1_stamp, 1),
            "slot 1 should be empty"
        );
    }

    /// Integration test: same scenario with actual channel operations
    #[test]
    fn test_same_stamp_different_slots_integration() {
        let drop_count = Arc::new(AtomicUsize::new(0));

        #[derive(Clone, Debug)]
        struct DropCounter(Arc<AtomicUsize>);
        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }

        {
            let (tx, rx) = crate::spsc::vyukov::channel::<DropCounter, 4>();

            // First lap: write 4, read 4
            for _ in 0..4 {
                tx.try_send(DropCounter(drop_count.clone())).unwrap();
            }
            for _ in 0..4 {
                let _ = rx.try_recv().unwrap();
            }

            // 4 items dropped via read
            assert_eq!(drop_count.load(std::sync::atomic::Ordering::SeqCst), 4);

            // Second lap: write only to slot 0
            tx.try_send(DropCounter(drop_count.clone())).unwrap();

            // Now slot 0 and slot 1 both have stamp = 5, but only slot 0 has data!
            // Drop channel - only slot 0's item should be dropped
            drop(tx);
            drop(rx);
        }

        // Total: 4 (first lap reads) + 1 (slot 0 from Channel::drop) = 5
        assert_eq!(drop_count.load(std::sync::atomic::Ordering::SeqCst), 5);
    }

    /// Test full channel drop (all slots have unread data)
    #[test]
    fn test_channel_drop_full() {
        let drop_count = Arc::new(AtomicUsize::new(0));

        #[derive(Clone, Debug)]
        struct DropCounter(Arc<AtomicUsize>);
        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }

        {
            let (tx, rx) = crate::spsc::vyukov::channel::<DropCounter, 4>();

            // Fill the buffer completely
            for _ in 0..4 {
                tx.try_send(DropCounter(drop_count.clone())).unwrap();
            }

            // No reads - all 4 should be dropped by Channel::drop
            drop(tx);
            drop(rx);
        }

        assert_eq!(drop_count.load(std::sync::atomic::Ordering::SeqCst), 4);
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
