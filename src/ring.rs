use std::{cell::UnsafeCell, mem::MaybeUninit, ptr};

/// # Safety
///
/// RingBuffer doesn't implement [`Drop`]: the wrapper is responsible for
/// tracking which slots have initialized data that needs dropping.
pub(crate) struct RingBuffer<T, const N: usize>([T; N])
where
    T: Storable;

impl<T, const N: usize> RingBuffer<T, N>
where
    T: Storable,
{
    const MODULO_MASK: usize = N - 1;
    const N_POWER_OF_2: bool = N.is_power_of_two();
    const N_POSITIVE: bool = N > 0;

    /// Maps a sequence number to a buffer index using bitwise AND.
    #[inline]
    pub(crate) const fn index(&self, seq: usize) -> usize {
        seq & Self::MODULO_MASK
    }

    /// Returns a reference to the value at the given index.
    #[inline]
    pub(crate) fn get(&self, index: usize) -> &T {
        &self.0[index]
    }

    /// Drops the value at the given index.
    ///
    /// # Safety
    /// - Index must be valid
    /// - Slot must contain initialized data
    #[inline]
    pub(crate) unsafe fn drop_in_place(&self, index: usize) {
        unsafe { self.0[index].drop_in_place() }
    }
}

#[allow(clippy::missing_safety_doc)]
pub trait Storable {
    type Item;
    unsafe fn write(&self, value: Self::Item);
    unsafe fn read(&self) -> Self::Item;
    unsafe fn drop_in_place(&self);
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
    unsafe fn drop_in_place(&self) {
        unsafe {
            ptr::drop_in_place((*self.get()).as_mut_ptr());
        }
    }
}

impl<T: Storable, const N: usize> From<[T; N]> for RingBuffer<T, N> {
    fn from(buffer: [T; N]) -> Self {
        assert!(Self::N_POWER_OF_2, "N must be power of 2");
        assert!(Self::N_POSITIVE, "N must be positive");
        Self(buffer)
    }
}
