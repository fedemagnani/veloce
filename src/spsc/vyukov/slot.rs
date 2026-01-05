use crate::ring::{RingBuffer, Storable};
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::AtomicUsize;
use std::{ptr, sync::atomic::Ordering};

/// A slot in the ring buffer with per-slot sequence stamp for [Vyukov](https://x.com/dvyukov)-style synchronization.
///
/// It can be used for Vyukov's algorithm where each slot has its own sequence stamp,
/// allowing sender and receiver to synchronize through slot state rather
/// than constantly loading each other's head/tail cursors.
pub struct Slot<T> {
    /// Sequence stamp for this slot. Protocol:
    /// - Initial: slot index (0, 1, 2, ..., N-1)
    /// - After write: tail + 1 (signals "data ready for reader")  
    /// - After read: head + N (signals "slot ready for next writer lap")
    pub(crate) stamp: AtomicUsize,
    /// The actual value storage
    value: UnsafeCell<MaybeUninit<T>>,
}

impl<T> Slot<T> {
    /// Creates a new slot with the given initial stamp (typically the slot index).
    #[inline]
    pub fn new(stamp: usize) -> Self {
        Self {
            stamp: AtomicUsize::new(stamp),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Loads the stamp with Acquire ordering.
    #[inline]
    pub fn load_stamp(&self) -> usize {
        self.stamp.load(Ordering::Acquire)
    }

    /// Stores the stamp with Release ordering.
    #[inline]
    pub fn store_stamp(&self, stamp: usize) {
        self.stamp.store(stamp, Ordering::Release);
    }
}

impl<T> Storable for Slot<T> {
    type Item = T;
    /// Writes a value to the slot.
    ///
    /// # Safety
    /// - Caller must ensure no concurrent access to this slot's value
    /// - Will overwrite any existing value without dropping it
    #[inline]
    unsafe fn write(&self, value: T) {
        unsafe { ptr::write((*self.value.get()).as_mut_ptr(), value) };
    }

    /// Reads the value from the slot.
    ///
    /// # Safety
    /// - Caller must ensure the slot contains initialized data
    /// - Caller must ensure no concurrent access to this slot's value
    #[inline]
    unsafe fn read(&self) -> T {
        unsafe { ptr::read((*self.value.get()).as_ptr()) }
    }

    /// Drops the value in place.
    ///
    /// # Safety
    /// - Caller must ensure the slot contains initialized data
    #[inline]
    unsafe fn drop_in_place(&self) {
        unsafe {
            ptr::drop_in_place((*self.value.get()).as_mut_ptr());
        }
    }
}

impl<T, const N: usize> Default for RingBuffer<Slot<T>, N> {
    fn default() -> Self {
        let slots = std::array::from_fn(|i| Slot::new(i));
        Self::from(slots)
    }
}

#[cfg(test)]
mod slot_test {
    use crate::ring::RingBuffer;

    use super::*;

    /// Test read and write in the buffer
    #[test]
    fn test_rw() {
        const N: usize = 2;
        let ring = RingBuffer::<Slot<i32>, N>::default();
        let seq = 0;
        let i = ring.index(seq);
        let val = 28392;
        unsafe {
            ring.get(i).write(val);
            let out = ring.get(i).read();
            assert_eq!(out, val)
        };
    }

    /// Ring buffer should not be constructed with N that is not power of two
    #[test]
    #[should_panic]
    fn test_panics() {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));

        const N: usize = 6;
        let _ = RingBuffer::<Slot<()>, N>::default();

        std::panic::set_hook(prev);
    }

    /// Verify that `MODULO_MASK` works to get the modulo of the given sequence number
    #[test]
    fn test_modulo() {
        const N: usize = 2;
        let ring = RingBuffer::<Slot<()>, N>::default();
        let i = 3;
        assert_eq!(ring.index(i), i % N);
        let i = 4;
        assert_eq!(ring.index(i), i % N);
    }

    /// Test initial stamp values
    #[test]
    fn test_initial_stamps() {
        const N: usize = 4;
        let ring = RingBuffer::<Slot<i32>, N>::default();
        for i in 0..N {
            assert_eq!(ring.get(i).load_stamp(), i);
        }
    }

    /// Test stamp update protocol
    #[test]
    fn test_stamp_protocol() {
        const N: usize = 4;
        let ring = RingBuffer::<Slot<i32>, N>::default();

        // Simulate sender writing to slot 0 at tail=0
        let slot = ring.get(0);
        assert_eq!(slot.load_stamp(), 0); // Initial: index
        unsafe { slot.write(42) };
        slot.store_stamp(1); // After write: tail + 1

        // Simulate receiver reading from slot 0 at head=0
        assert_eq!(slot.load_stamp(), 1); // Should see "data ready"
        let val = unsafe { slot.read() };
        assert_eq!(val, 42);
        slot.store_stamp(N); // After read: head + N (ready for next lap)

        // Simulate sender writing again at tail=N (wrap around)
        assert_eq!(slot.load_stamp(), N); // Should see "ready for write"
    }
}
