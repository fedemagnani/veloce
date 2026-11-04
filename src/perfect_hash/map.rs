// Port of ConstexprCore/perfect_hash by Daniel Lemire and Francisco Thiesen, MIT

use core::mem::MaybeUninit;

use super::generator::Phf;
use super::set::PerfectSet;

/// Map from up to 255 keys of up to 254 bytes, hashed perfectly at compile time: see the [module docs](super)
pub struct PerfectMap<V, const N: usize, const T: usize, const L: usize, const W: usize> {
    set: PerfectSet<N, T, L, W>,
    // slot order, one load from the matched slot; declaration order for single-byte keys, found by index
    values: [MaybeUninit<V>; T],
}

impl<V, const N: usize, const T: usize, const L: usize, const W: usize> PerfectMap<V, N, T, L, W> {
    /// Builds the map from its keys, their values and a perfect hash computed for the keys
    pub const fn new(keys: &[&[u8]; N], values: [V; N], phf: &Phf) -> Self {
        let set = PerfectSet::new(keys, phf);
        let values = MaybeUninit::new(values);
        let first_value = values.as_ptr().cast::<V>();
        let mut slots = [const { MaybeUninit::uninit() }; T];
        let mut i = 0;
        while i < N {
            let index = Self::value_index(&set, i);
            // SAFETY: `i < N` reads each value of the array exactly once, moving it out of `values`
            let value = unsafe { first_value.add(i).read() };
            slots[index] = MaybeUninit::new(value);
            i += 1;
        }
        Self { set, values: slots }
    }

    /// Where the value of the key declared at `i` is stored
    const fn value_index(set: &PerfectSet<N, T, L, W>, i: usize) -> usize {
        if L == 1 { i } else { set.key_to_slot(i) }
    }

    /// Value of `key`, if in the map
    #[inline(always)]
    pub fn get<K: AsRef<[u8]> + ?Sized>(&self, key: &K) -> Option<&V> {
        let index = if L == 1 {
            self.set.index_of(key)?
        } else {
            self.set.slot_match(key.as_ref())?
        };
        // SAFETY: a matched slot, or a declaration index for single-byte keys, holds an initialized value
        let value = unsafe { self.values.get_unchecked(index).assume_init_ref() };
        Some(value)
    }

    /// Whether `key` is in the map
    #[inline(always)]
    pub fn contains_key<K: AsRef<[u8]> + ?Sized>(&self, key: &K) -> bool {
        self.set.contains(key)
    }
}

impl<V, const N: usize, const T: usize, const L: usize, const W: usize> Drop
    for PerfectMap<V, N, T, L, W>
{
    fn drop(&mut self) {
        let mut i = 0;
        while i < N {
            let index = Self::value_index(&self.set, i);
            // SAFETY: each of the N values sits at its own index and is dropped once
            unsafe { self.values[index].assume_init_drop() };
            i += 1;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl<V, const N: usize, const T: usize, const L: usize, const W: usize> PerfectMap<V, N, T, L, W> {
        /// Number of keys
        pub(crate) const fn len(&self) -> usize {
            N
        }
    }
}
