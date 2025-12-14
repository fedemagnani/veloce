use std::{cell::UnsafeCell, mem::MaybeUninit, ptr};

#[allow(clippy::missing_safety_doc)]
pub trait Storable {
    type Item;
    unsafe fn write(&self, value: Self::Item);
    unsafe fn read(&self) -> Self::Item;
    unsafe fn drop(&self);
}

impl<T> Storable for UnsafeCell<MaybeUninit<T>> {
    type Item = T;

    /// # Safety:
    ///
    /// this will overwrite any existing value without dropping it, causing a potential memory leak
    #[inline]
    unsafe fn write(&self, value: T) {
        unsafe { ptr::write((*self.get()).as_mut_ptr(), value) };
    }

    #[inline]
    unsafe fn read(&self) -> Self::Item {
        unsafe { ptr::read((*self.get()).as_ptr()) }
    }

    /// # Safety:
    ///
    /// the slot expects intialized data
    #[inline]
    unsafe fn drop(&self) {
        unsafe {
            ptr::drop_in_place((*self.get()).as_mut_ptr());
        }
    }
}

/// # Safety:
///
/// RingBuffer doesn't have any implementation of [`Drop`]: it is responsibility of the
/// wrapper of [`RingBuffer`] monitoring which memory slots have been initialized and would require drop
pub(crate) struct RingBuffer<T, const N: usize>([T; N])
where
    T: Storable;

impl<T: Storable, const N: usize> From<[T; N]> for RingBuffer<T, N> {
    fn from(buffer: [T; N]) -> Self {
        assert!(Self::N_POWER_OF_2, "N must be power of 2");
        assert!(Self::N_POSITIVE, "N must be positive");
        Self(buffer)
    }
}

impl<T: Storable, const N: usize> RingBuffer<T, N> {
    const MODULO_MASK: usize = N - 1;
    const N_POWER_OF_2: bool = N.is_power_of_two();
    const N_POSITIVE: bool = N > 0;

    #[inline]
    pub(crate) const fn index(&self, seq: usize) -> usize {
        seq & Self::MODULO_MASK
    }

    /// # Safety
    ///
    /// - `i` is assumed to be an index of the inner slice
    #[inline]
    pub(crate) unsafe fn write(&self, i: usize, value: T::Item) {
        let cell = &self.0[i];
        unsafe { cell.write(value) };
    }

    /// # Safety
    ///
    /// - `i` is assumed to be an index of the inner slice
    #[inline]
    pub(crate) unsafe fn read(&self, i: usize) -> T::Item {
        let cell = &self.0[i];
        unsafe { cell.read() }
    }

    /// # Safety
    ///
    /// - `i` is assumed to be an index of the inner slice
    #[inline]
    pub(crate) unsafe fn drop(&self, i: usize) {
        let cell = &self.0[i];
        unsafe { cell.drop() }
    }
}

impl<T, const N: usize> Default for RingBuffer<UnsafeCell<MaybeUninit<T>>, N> {
    fn default() -> Self {
        let buffer = [const { UnsafeCell::new(MaybeUninit::uninit()) }; N];
        buffer.into()
    }
}

#[cfg(test)]
mod ring_test {
    use super::*;

    /// Test read and write in the buffer
    #[test]
    fn test_rw() {
        const N: usize = 2;
        let ring = RingBuffer::<_, N>::default();
        let seq = 0;
        let i = ring.index(seq);
        let val = 28392;
        unsafe {
            ring.write(i, val);
            let out = ring.read(i);
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
        type R = RingBuffer<UnsafeCell<MaybeUninit<()>>, N>;
        R::default();

        std::panic::set_hook(prev);
    }

    /// Verify that `MODULO_MASK` works to get the modulo of the given sequence number
    #[test]
    fn test_modulo() {
        const N: usize = 2;
        type R = RingBuffer<UnsafeCell<MaybeUninit<()>>, N>;
        let i = 3;
        assert_eq!(i & R::MODULO_MASK, i % N);
        let i = 4;
        assert_eq!(i & R::MODULO_MASK, i % N);

        const N_F: usize = 5;
        type RF = RingBuffer<UnsafeCell<MaybeUninit<()>>, N_F>;
        let i = 3;
        assert_ne!(i & RF::MODULO_MASK, i % N_F);
        let i = 6;
        assert_ne!(i & RF::MODULO_MASK, i % N_F);
    }
}
