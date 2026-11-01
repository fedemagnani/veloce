// Port of ConstexprCore/perfect_hash by Daniel Lemire and Francisco Thiesen, MIT

use super::generator::{
    FULLHASH_MODE, HD_HASH_2BYTE_FLAG, HD_MODE, LAST_CHAR, MAX_POSITIONS, MAX_TABLE_SIZE, Phf,
    fh_hash, hd_bucket_hash, hd_key_hash_2, hd_key_hash_4, padded_len, safe_byte,
};
#[cfg(all(
    any(
        all(target_arch = "aarch64", target_feature = "neon"),
        all(target_arch = "x86_64", target_feature = "sse2")
    ),
    not(miri)
))]
use super::simd;

/// Position byte standing for the last byte of the key
const POS_LAST_CHAR: u8 = 0xFF;
/// Direct-table entry of a byte that is no key
const NO_KEY: u8 = 0xFF;

/// Set of up to 255 keys of up to 254 bytes, hashed perfectly at compile time: see the [module docs](super)
#[repr(C)]
pub struct PerfectSet<const N: usize, const T: usize, const L: usize, const W: usize> {
    // hot data, read on every lookup
    asso_values: [[u8; 256]; MAX_POSITIONS],
    num_positions: u8,
    positions: [u8; MAX_POSITIONS],
    min_key_len: u8,
    hash_seed: u64,
    // 0 marks an empty slot: the empty key, when a member, always hashes to its own slot
    slot_key_len: [u8; T],
    slot_key_data: [[u8; W]; T],
    packed_keys: [u64; T],
    packed_keys2: [u64; T],
    // cold data
    slot_to_key: [u8; T],
    key_to_slot: [u8; N],
    // single-byte sets only: byte to declaration index
    direct_lookup: [u8; 256],
}

impl<const N: usize, const T: usize, const L: usize, const W: usize> PerfectSet<N, T, L, W> {
    /// Builds the set from its keys and a perfect hash computed for them, panicking (at compile time) on a mismatch
    pub const fn new(keys: &[&[u8]; N], phf: &Phf) -> Self {
        assert!(
            N > 0 && N <= 255,
            "perfect_hash: the key count must be in 1..=255"
        );
        assert!(
            T >= N && T <= MAX_TABLE_SIZE,
            "perfect_hash: the table size must be in N..=256"
        );
        assert!(
            L >= 1 && L < 255,
            "perfect_hash: the longest key must be shorter than 255 bytes"
        );
        assert!(
            W == padded_len(L),
            "perfect_hash: W must be L rounded up to a multiple of 16"
        );
        assert!(
            phf.num_keys == N,
            "perfect_hash: the hash was computed for another key count"
        );
        assert!(
            phf.table_size == T,
            "perfect_hash: the hash was computed for another table size"
        );
        let mut i = 0;
        while i < N {
            assert!(keys[i].len() <= L, "perfect_hash: key longer than L");
            i += 1;
        }

        let mut set = Self {
            asso_values: [[0; 256]; MAX_POSITIONS],
            num_positions: phf.num_positions as u8,
            positions: [0; MAX_POSITIONS],
            min_key_len: 0,
            hash_seed: phf.seed,
            slot_key_len: [0; T],
            slot_key_data: [[0; W]; T],
            packed_keys: [0; T],
            packed_keys2: [0; T],
            slot_to_key: [0; T],
            key_to_slot: [0; N],
            direct_lookup: [NO_KEY; 256],
        };

        // displacement modes keep one raw table, below 256 already; gperf tables reduce mod T
        let displacement = phf.num_positions == HD_MODE || phf.num_positions == FULLHASH_MODE;
        let tables = if displacement { 1 } else { phf.num_positions };
        let mut p = 0;
        while p < tables {
            let mut c = 0;
            while c < 256 {
                let value = phf.asso_values[p][c];
                set.asso_values[p][c] = if displacement {
                    value as u8
                } else {
                    (value % T) as u8
                };
                c += 1;
            }
            p += 1;
        }
        let num_positions = match phf.num_positions {
            HD_MODE => 1,
            FULLHASH_MODE => 0,
            n => n,
        };
        let mut p = 0;
        while p < num_positions {
            let pos = phf.positions[p];
            set.positions[p] = if pos == LAST_CHAR {
                POS_LAST_CHAR
            } else {
                pos as u8
            };
            p += 1;
        }
        let mut s = 0;
        while s < T {
            set.slot_to_key[s] = phf.slot_to_key[s] as u8;
            s += 1;
        }

        let mut i = 0;
        while i < N {
            let slot = set.compute_hash(keys[i]);
            assert!(
                set.slot_to_key[slot] as usize == i,
                "perfect_hash: the hash does not map the keys"
            );
            i += 1;
        }
        set.init_inline_data(keys);
        set
    }

    /// Whether `key` is in the set
    #[inline(always)]
    pub fn contains<K: AsRef<[u8]> + ?Sized>(&self, key: &K) -> bool {
        let key = key.as_ref();
        if L == 1 {
            if let [byte] = key {
                return self.direct_lookup[*byte as usize] != NO_KEY;
            }
            if key.len() > 1 {
                return false;
            }
        }
        // `min_key_len` is constant for a static set, so this branch folds away
        if self.min_key_len > 0 && key.is_empty() {
            return false;
        }
        let len = key.len();
        // compares never read past L bytes; longer keys fail the length check
        let probe = &key[..len.min(L)];
        let slot = self.compute_hash(key);
        let len_ok = self.slot_key_len[slot] as usize == len;
        let key_ok = self.compare_key(probe, slot);
        len_ok & key_ok
    }

    /// Declaration index of `key`, if in the set
    #[inline(always)]
    pub fn index_of<K: AsRef<[u8]> + ?Sized>(&self, key: &K) -> Option<usize> {
        let key = key.as_ref();
        if L == 1 {
            if self.min_key_len > 0 {
                // branchless: a length other than 1 forces the no-key sentinel instead of branching
                let len = key.len();
                let byte = safe_byte(key, 0);
                let mismatch_mask = ((len != 1) as u8).wrapping_neg();
                let idx = self.direct_lookup[byte as usize] | mismatch_mask;
                return if idx == NO_KEY {
                    None
                } else {
                    Some(idx as usize)
                };
            }
            if let [byte] = key {
                let idx = self.direct_lookup[*byte as usize];
                return if idx == NO_KEY {
                    None
                } else {
                    Some(idx as usize)
                };
            }
            if key.len() > 1 {
                return None;
            }
        }
        let slot = self.slot_match(key)?;
        Some(self.slot_to_key[slot] as usize)
    }

    /// Slot of `key`, if in the set, one dependent load short of `index_of`
    #[inline(always)]
    pub(crate) fn slot_match(&self, key: &[u8]) -> Option<usize> {
        if self.min_key_len > 0 && key.is_empty() {
            return None;
        }
        let len = key.len();
        let probe = &key[..len.min(L)];
        let slot = self.compute_hash(key);
        // compare keys first, which keeps misses free of mispredictions
        if !self.compare_key(probe, slot) {
            return None;
        }
        if self.slot_key_len[slot] as usize != len {
            return None;
        }
        Some(slot)
    }

    /// Slot of the key declared at `i`
    pub(crate) const fn key_to_slot(&self, i: usize) -> usize {
        self.key_to_slot[i] as usize
    }

    /// Hash slot of `key`
    #[codegen::watch]
    #[inline(always)]
    pub(crate) const fn compute_hash(&self, key: &[u8]) -> usize {
        // one never-taken branch gates both displacement modes
        if self.num_positions as usize >= FULLHASH_MODE {
            if self.num_positions as usize == FULLHASH_MODE {
                let h = fh_hash(key, self.hash_seed);
                let bucket = ((h >> 48) & 0xFF) as usize;
                let displacement = self.asso_values[0][bucket] as usize;
                return Self::reduce(displacement.wrapping_add(h as usize));
            }
            let bucket = hd_bucket_hash(key);
            let two_bytes = self.positions[0] as usize == HD_HASH_2BYTE_FLAG;
            let key_hash = if two_bytes {
                hd_key_hash_2(key)
            } else {
                hd_key_hash_4(key)
            };
            let displacement = self.asso_values[0][bucket] as usize;
            return Self::reduce(displacement.wrapping_add(key_hash));
        }

        let klen = key.len();
        let mut h = klen;
        let mut i = 0;
        // large or long-key sets mispredict per-position length branches on shuffled streams, so mask instead
        if N >= 64 || L > 16 {
            while i < self.num_positions as usize {
                let pos = self.positions[i];
                let idx = if pos == POS_LAST_CHAR {
                    klen.wrapping_sub(1)
                } else {
                    pos as usize
                };
                let has = ((idx < klen) as usize).wrapping_neg();
                let byte = safe_byte(key, idx);
                h += self.asso_values[i][byte as usize] as usize & has;
                i += 1;
            }
        } else {
            while i < self.num_positions as usize {
                let pos = self.positions[i];
                let byte = if pos == POS_LAST_CHAR {
                    key.last()
                } else if (pos as usize) < klen {
                    Some(&key[pos as usize])
                } else {
                    None
                };
                if let Some(&byte) = byte {
                    h += self.asso_values[i][byte as usize] as usize;
                }
                i += 1;
            }
        }
        Self::reduce(h)
    }

    /// Slot of a key with `T` a power of two or not
    #[inline(always)]
    const fn reduce(h: usize) -> usize {
        if T.is_power_of_two() {
            h & (T - 1)
        } else {
            h % T
        }
    }

    /// Minimum length, inline keys and their packed forms, and the single-byte direct table
    const fn init_inline_data(&mut self, keys: &[&[u8]; N]) {
        let mut min = keys[0].len();
        let mut i = 1;
        while i < N {
            if keys[i].len() < min {
                min = keys[i].len();
            }
            i += 1;
        }
        self.min_key_len = min as u8;

        let mut s = 0;
        while s < T {
            let k = self.slot_to_key[s] as usize;
            if k < N {
                let key = keys[k];
                self.slot_key_len[s] = key.len() as u8;
                let mut c = 0;
                while c < key.len() {
                    self.slot_key_data[s][c] = key[c];
                    c += 1;
                }
                self.key_to_slot[k] = s as u8;
                self.packed_keys[s] = Self::pack_stored(key, 0);
                self.packed_keys2[s] = Self::pack_stored(key, 8);
                if L <= 4 && !key.is_empty() {
                    self.packed_keys[s] = Self::overlap_encoding(key);
                }
            }
            s += 1;
        }

        if L == 1 {
            let mut i = 0;
            while i < N {
                if let Some(&byte) = keys[i].first() {
                    self.direct_lookup[byte as usize] = i as u8;
                }
                i += 1;
            }
        }
    }

    /// Key bytes `from..from + 8`, little-endian and zero-padded
    const fn pack_stored(key: &[u8], from: usize) -> u64 {
        let mut packed = 0;
        let mut c = from;
        while c < key.len() && c < from + 8 {
            packed |= (key[c] as u64) << ((c - from) * 8);
            c += 1;
        }
        packed
    }

    /// First and last two bytes plus the length, which covers every byte of keys up to 4 bytes
    #[inline(always)]
    const fn overlap_encoding(key: &[u8]) -> u64 {
        let len = key.len();
        if len >= 2 {
            let lo = u16::from_le_bytes([key[0], key[1]]) as u64;
            let hi = u16::from_le_bytes([key[len - 2], key[len - 1]]) as u64;
            lo | (hi << 16) | ((len as u64) << 32)
        } else {
            key[0] as u64 | (1 << 32)
        }
    }

    /// Whether the probe, at most L bytes, equals the key stored at `slot`, the length checked apart
    #[codegen::watch]
    #[inline(always)]
    fn compare_key(&self, probe: &[u8], slot: usize) -> bool {
        if probe.is_empty() {
            return true;
        }
        if L <= 4 {
            // two in-bounds halfword loads cover every byte of keys up to 4 bytes
            return Self::overlap_encoding(probe) == self.packed_keys[slot];
        }
        if L <= 8 {
            return self.compare_8(probe, slot);
        }
        if L <= 16 {
            return self.compare_16(probe, slot);
        }
        self.compare_long(probe, slot)
    }

    #[cfg(all(
        any(
            all(target_arch = "aarch64", target_feature = "neon"),
            all(target_arch = "x86_64", target_feature = "sse2")
        ),
        not(miri)
    ))]
    #[inline(always)]
    fn compare_8(&self, probe: &[u8], slot: usize) -> bool {
        simd::compare_8(probe, self.packed_keys[slot])
    }

    #[cfg(all(
        any(
            all(target_arch = "aarch64", target_feature = "neon"),
            all(target_arch = "x86_64", target_feature = "sse2")
        ),
        not(miri)
    ))]
    #[inline(always)]
    fn compare_16(&self, probe: &[u8], slot: usize) -> bool {
        simd::compare_16(probe, &self.slot_key_data[slot])
    }

    #[cfg(all(
        any(
            all(target_arch = "aarch64", target_feature = "neon"),
            all(target_arch = "x86_64", target_feature = "sse2")
        ),
        not(miri)
    ))]
    #[inline(always)]
    fn compare_long(&self, probe: &[u8], slot: usize) -> bool {
        simd::compare_chunks(probe, &self.slot_key_data[slot])
    }

    #[cfg(not(all(
        any(
            all(target_arch = "aarch64", target_feature = "neon"),
            all(target_arch = "x86_64", target_feature = "sse2")
        ),
        not(miri)
    )))]
    #[inline(always)]
    fn compare_8(&self, probe: &[u8], slot: usize) -> bool {
        Self::pack_input(probe) == self.packed_keys[slot]
    }

    /// Two packed words, branch-free
    #[cfg(not(all(
        any(
            all(target_arch = "aarch64", target_feature = "neon"),
            all(target_arch = "x86_64", target_feature = "sse2")
        ),
        not(miri)
    )))]
    #[inline(always)]
    fn compare_16(&self, probe: &[u8], slot: usize) -> bool {
        let first = self.pack_first_8(probe);
        let mut second = 0;
        let mut i = 8;
        while i < L {
            second |= (safe_byte(probe, i) as u64) << (8 * (i - 8));
            i += 1;
        }
        let diff = (first ^ self.packed_keys[slot]) | (second ^ self.packed_keys2[slot]);
        diff == 0
    }

    /// The first packed word, then every byte up to L, branch-free
    #[cfg(not(all(
        any(
            all(target_arch = "aarch64", target_feature = "neon"),
            all(target_arch = "x86_64", target_feature = "sse2")
        ),
        not(miri)
    )))]
    #[inline(always)]
    fn compare_long(&self, probe: &[u8], slot: usize) -> bool {
        let stored = &self.slot_key_data[slot];
        let mut diff = self.pack_first_8(probe) ^ self.packed_keys[slot];
        let mut i = 8;
        while i < L {
            diff |= (stored[i] ^ safe_byte(probe, i)) as u64;
            i += 1;
        }
        diff == 0
    }

    /// First 8 probe bytes, one load when every key is at least that long
    #[cfg(not(all(
        any(
            all(target_arch = "aarch64", target_feature = "neon"),
            all(target_arch = "x86_64", target_feature = "sse2")
        ),
        not(miri)
    )))]
    #[inline(always)]
    fn pack_first_8(&self, probe: &[u8]) -> u64 {
        match probe.first_chunk::<8>() {
            Some(chunk) if self.min_key_len >= 8 => u64::from_le_bytes(*chunk),
            _ => Self::pack_input(probe),
        }
    }

    /// Up to 8 probe bytes, little-endian and zero-padded, without branches
    #[cfg(not(all(
        any(
            all(target_arch = "aarch64", target_feature = "neon"),
            all(target_arch = "x86_64", target_feature = "sse2")
        ),
        not(miri)
    )))]
    #[inline(always)]
    fn pack_input(probe: &[u8]) -> u64 {
        let mut packed = probe[0] as u64;
        let mut i = 1;
        while i < L && i < 8 {
            packed |= (safe_byte(probe, i) as u64) << (8 * i);
            i += 1;
        }
        packed
    }
}

#[cfg(test)]
mod test {
    use super::super::generator::test::Algorithm;
    use super::*;

    impl<const N: usize, const T: usize, const L: usize, const W: usize> PerfectSet<N, T, L, W> {
        /// Number of keys
        pub(crate) const fn len(&self) -> usize {
            N
        }

        /// Number of hash slots
        pub(crate) const fn table_size(&self) -> usize {
            T
        }

        /// Hash family the generator settled on
        pub(crate) const fn algorithm(&self) -> Algorithm {
            Algorithm::from_num_positions(self.num_positions as usize)
        }

        /// Key declared at index `i`
        pub(crate) fn key_at(&self, i: usize) -> &[u8] {
            let slot = self.key_to_slot[i] as usize;
            let len = self.slot_key_len[slot] as usize;
            &self.slot_key_data[slot][..len]
        }
    }
}
