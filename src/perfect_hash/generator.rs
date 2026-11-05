// Port of ConstexprCore/perfect_hash by Daniel Lemire and Francisco Thiesen, MIT

//! Compile-time search for a perfect hash: three families tried as fallbacks, cheapest lookup first, first fit wins

/// Byte positions the gperf-form hash may read, each costing a 256-byte table in every set
pub(crate) const MAX_POSITIONS: usize = 16;
/// Largest table the byte-indexed container can hold
pub(crate) const MAX_TABLE_SIZE: usize = 256;
/// Position standing for the last byte of the key, whatever its length
pub(crate) const LAST_CHAR: usize = usize::MAX;
/// `num_positions` sentinel of the Hash-and-Displace tier
pub(crate) const HD_MODE: usize = 0xFF;
/// `num_positions` sentinel of the seeded whole-key-hash tier
pub(crate) const FULLHASH_MODE: usize = 0xFE;
/// Hash-and-Displace key hash over the first 2 bytes, stored in `positions[0]`
pub(crate) const HD_HASH_2BYTE_FLAG: usize = 2;
/// Hash-and-Displace key hash over the first 4 bytes, stored in `positions[0]`
pub(crate) const HD_HASH_4BYTE_FLAG: usize = 4;
/// Returned by the position search when no position set within `MAX_POSITIONS` distinguishes all keys
const POSITION_SEARCH_FAILED: usize = usize::MAX - 1;
/// Capped evaluations of `count_undistinguished_pairs` in the backtracking search
const BACKTRACKING_BUDGET: usize = 5000;
/// Deepest position set the backtracking search explores
const MAX_DEPTH: usize = if MAX_POSITIONS < 8 { MAX_POSITIONS } else { 8 };
/// Seeds tried by the whole-key-hash tier, observed seeds being almost always 0-2
const MAX_SEEDS: u64 = 512;
/// Multiplier of the whole-key hash
const FH_MUL: u64 = 0x9E37_79B9_7F4A_7C15;

/// A perfect hash for a key set: table size, hash family and its parameters
#[derive(Debug, Clone, Copy)]
pub struct Phf {
    pub(crate) num_keys: usize,
    pub(crate) table_size: usize,
    pub(crate) asso_values: [[usize; 256]; MAX_POSITIONS],
    pub(crate) num_positions: usize,
    pub(crate) positions: [usize; MAX_POSITIONS],
    pub(crate) slot_to_key: [usize; MAX_TABLE_SIZE],
    pub(crate) seed: u64,
}

impl Phf {
    /// Searches gperf, then Hash-and-Displace, then seeded whole-key hashing, at power-of-two sizes up to 256
    pub const fn new<const N: usize>(keys: &[&[u8]; N]) -> Self {
        assert!(N > 0, "perfect_hash: the key set is empty");
        assert!(N <= 255, "perfect_hash: more than 255 keys");
        let mut i = 0;
        while i < N {
            let mut j = i + 1;
            while j < N {
                assert!(
                    !bytes_eq(keys[i], keys[j]),
                    "perfect_hash: duplicate key in key set"
                );
                j += 1;
            }
            i += 1;
        }

        let start = next_power_of_2(N);
        let cap = if start * 8 < MAX_TABLE_SIZE {
            start * 8
        } else {
            MAX_TABLE_SIZE
        };
        // pairs needing position coverage only grow as the power-of-two size shrinks, so failing at cap fails everywhere
        let mut pre_positions = [0; MAX_POSITIONS];
        let positions_possible =
            select_positions(keys, &mut pre_positions, cap) != POSITION_SEARCH_FAILED;
        if positions_possible {
            let mut m = start;
            while m <= cap {
                if let Some(phf) = try_generate_gperf(keys, m) {
                    return phf;
                }
                m *= 2;
            }
        }
        let mut m = start;
        while m <= cap {
            if let Some(phf) = try_hash_and_displace(keys, m) {
                return phf;
            }
            m *= 2;
        }
        let mut m = start;
        while m <= cap {
            if let Some(phf) = try_seeded_fullhash(keys, m) {
                return phf;
            }
            m *= 2;
        }
        panic!("perfect_hash: no perfect hash fits in <= 256 slots for this key set");
    }

    /// Number of slots of the table, at least the number of keys
    pub const fn table_size(&self) -> usize {
        self.table_size
    }

    /// Empty result for `n` keys and a table of `m` slots
    const fn empty(n: usize, m: usize) -> Self {
        Self {
            num_keys: n,
            table_size: m,
            asso_values: [[0; 256]; MAX_POSITIONS],
            num_positions: 0,
            positions: [0; MAX_POSITIONS],
            slot_to_key: [n; MAX_TABLE_SIZE],
            seed: 0,
        }
    }
}

/// Length of the longest key
pub const fn max_key_len<const N: usize>(keys: &[&[u8]; N]) -> usize {
    let mut max = 0;
    let mut i = 0;
    while i < N {
        if keys[i].len() > max {
            max = keys[i].len();
        }
        i += 1;
    }
    max
}

/// Bytes of a stored key: the longest key length rounded up to whole 16-byte chunks
pub const fn padded_len(max_key_len: usize) -> usize {
    let len = if max_key_len == 0 { 1 } else { max_key_len };
    len.div_ceil(16) * 16
}

/// Byte-wise equality, `PartialEq` not being callable in const context
const fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn next_power_of_2(n: usize) -> usize {
    let mut p = 1;
    while p < n {
        p <<= 1;
    }
    p
}

/// Selects byte positions distinguishing every pair of keys the length term can't separate
const fn select_positions<const N: usize>(
    keys: &[&[u8]; N],
    positions: &mut [usize; MAX_POSITIONS],
    modulus: usize,
) -> usize {
    // strategy 1: lengths alone
    if positions_distinguish(keys, positions, 0, modulus) {
        return 0;
    }

    // candidates 0..max_len plus LAST_CHAR, ranked by discriminating power
    const MAX_CANDIDATES: usize = 256;
    let max_len = max_key_len(keys);
    let mut candidates = [0; MAX_CANDIDATES];
    let mut powers = [0; MAX_CANDIDATES];
    let mut num_candidates = 0;
    let mut p = 0;
    while p < max_len && num_candidates < MAX_CANDIDATES - 1 {
        candidates[num_candidates] = p;
        powers[num_candidates] = discriminating_power(keys, p, modulus);
        num_candidates += 1;
        p += 1;
    }
    candidates[num_candidates] = LAST_CHAR;
    powers[num_candidates] = discriminating_power(keys, LAST_CHAR, modulus);
    num_candidates += 1;

    let mut i = 0;
    while i < num_candidates {
        let mut j = i + 1;
        while j < num_candidates {
            if powers[j] > powers[i] {
                candidates.swap(i, j);
                powers.swap(i, j);
            }
            j += 1;
        }
        i += 1;
    }

    // strategy 2: the single best position
    positions[0] = candidates[0];
    if positions_distinguish(keys, positions, 1, modulus) {
        return 1;
    }

    // strategy 3: first and last byte
    if MAX_POSITIONS >= 2 {
        positions[0] = 0;
        positions[1] = LAST_CHAR;
        if positions_distinguish(keys, positions, 2, modulus) {
            return 2;
        }
    }

    // strategy 4: backtracking over the top candidates
    let mut budget = BACKTRACKING_BUDGET;
    if let Some(found) = backtracking_search(
        keys,
        &candidates,
        num_candidates,
        positions,
        &mut budget,
        modulus,
    ) {
        return found;
    }

    // strategy 5: greedily add candidates until every pair is distinguished
    let mut num_positions = 0;
    let mut ci = 0;
    while ci < num_candidates && num_positions < MAX_POSITIONS {
        let mut already = false;
        let mut p = 0;
        while p < num_positions {
            if positions[p] == candidates[ci] {
                already = true;
                break;
            }
            p += 1;
        }
        if !already {
            positions[num_positions] = candidates[ci];
            num_positions += 1;
            if positions_distinguish(keys, positions, num_positions, modulus) {
                return num_positions;
            }
        }
        ci += 1;
    }
    POSITION_SEARCH_FAILED
}

/// Whether the positions separate every pair of keys whose lengths agree mod `modulus`
const fn positions_distinguish<const N: usize>(
    keys: &[&[u8]; N],
    positions: &[usize],
    num_positions: usize,
    modulus: usize,
) -> bool {
    let mut i = 0;
    while i < N {
        let mut j = i + 1;
        while j < N {
            let same_len = keys[i].len() % modulus == keys[j].len() % modulus;
            if same_len && !pair_distinguished(keys[i], keys[j], positions, num_positions) {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

/// Whether two keys differ at one of the positions, needed only when their lengths agree mod `modulus`
const fn pair_distinguished(a: &[u8], b: &[u8], positions: &[usize], num_positions: usize) -> bool {
    let mut p = 0;
    while p < num_positions {
        if char_at(a, positions[p]) != char_at(b, positions[p]) {
            return true;
        }
        p += 1;
    }
    false
}

/// Byte at `pos` (`LAST_CHAR` for the last one), or 256 when the key is too short
const fn char_at(key: &[u8], pos: usize) -> usize {
    if pos == LAST_CHAR {
        if key.is_empty() {
            return 256;
        }
        return key[key.len() - 1] as usize;
    }
    if pos >= key.len() {
        return 256;
    }
    key[pos] as usize
}

/// Number of distinct `(length % modulus, char_at(key, pos))` pairs
const fn discriminating_power<const N: usize>(
    keys: &[&[u8]; N],
    pos: usize,
    modulus: usize,
) -> usize {
    let mut len_mods = [0; N];
    let mut chars = [0; N];
    let mut i = 0;
    while i < N {
        len_mods[i] = keys[i].len() % modulus;
        chars[i] = char_at(keys[i], pos);
        i += 1;
    }
    let mut count = 0;
    let mut i = 0;
    while i < N {
        let mut dup = false;
        let mut j = 0;
        while j < i {
            if len_mods[i] == len_mods[j] && chars[i] == chars[j] {
                dup = true;
                break;
            }
            j += 1;
        }
        if !dup {
            count += 1;
        }
        i += 1;
    }
    count
}

/// Bounded DFS for a minimal distinguishing position set among the top 20 candidates
const fn backtracking_search<const N: usize>(
    keys: &[&[u8]; N],
    candidates: &[usize],
    num_candidates: usize,
    positions: &mut [usize; MAX_POSITIONS],
    budget: &mut usize,
    modulus: usize,
) -> Option<usize> {
    let breadth = if num_candidates < 20 {
        num_candidates
    } else {
        20
    };
    let mut stack = [Frame {
        next_ci: 0,
        parent_count: 0,
    }; MAX_DEPTH + 1];
    let mut sp = 0;

    let initial_count = count_undistinguished_pairs(keys, positions, 0, modulus);
    if *budget > 0 {
        *budget -= 1;
    }
    if initial_count == 0 {
        return Some(0);
    }
    stack[0] = Frame {
        next_ci: 0,
        parent_count: initial_count,
    };

    while *budget > 0 {
        if sp > MAX_DEPTH {
            if sp == 0 {
                break;
            }
            sp -= 1;
            stack[sp].next_ci += 1;
            continue;
        }
        let frame = stack[sp];
        if frame.next_ci >= breadth {
            if sp == 0 {
                break;
            }
            sp -= 1;
            stack[sp].next_ci += 1;
            continue;
        }

        positions[sp] = candidates[frame.next_ci];
        *budget -= 1;
        let new_count = count_undistinguished_pairs(keys, positions, sp + 1, modulus);
        if new_count == 0 {
            return Some(sp + 1);
        }
        if new_count < frame.parent_count && sp + 1 < MAX_DEPTH {
            stack[sp + 1] = Frame {
                next_ci: frame.next_ci + 1,
                parent_count: new_count,
            };
            sp += 1;
        } else {
            stack[sp].next_ci += 1;
        }
    }
    None
}

/// Frame of the iterative backtracking search
#[derive(Clone, Copy)]
struct Frame {
    next_ci: usize,
    parent_count: usize,
}

/// Number of pairs the positions leave undistinguished
const fn count_undistinguished_pairs<const N: usize>(
    keys: &[&[u8]; N],
    positions: &[usize],
    num_positions: usize,
    modulus: usize,
) -> usize {
    let mut count = 0;
    let mut i = 0;
    while i < N {
        let mut j = i + 1;
        while j < N {
            let same_len = keys[i].len() % modulus == keys[j].len() % modulus;
            if same_len && !pair_distinguished(keys[i], keys[j], positions, num_positions) {
                count += 1;
            }
            j += 1;
        }
        i += 1;
    }
    count
}

/// gperf-inspired partition solver: fixes one symbol at a time, keeping equivalence classes collision-free
const fn try_generate_gperf<const N: usize>(keys: &[&[u8]; N], m: usize) -> Option<Phf> {
    let mut phf = Phf::empty(N, m);
    let num_positions = select_positions(keys, &mut phf.positions, m);
    if num_positions == POSITION_SEARCH_FAILED {
        return None;
    }
    phf.num_positions = num_positions;

    if num_positions == 0 {
        let mut lengths = [0; N];
        let mut i = 0;
        while i < N {
            lengths[i] = keys[i].len();
            i += 1;
        }
        return if place_keys(&mut phf, &lengths) {
            Some(phf)
        } else {
            None
        };
    }

    // each key's byte at each selected position
    let mut kchars = [[0; MAX_POSITIONS]; N];
    let mut k = 0;
    while k < N {
        let mut p = 0;
        while p < num_positions {
            kchars[k][p] = char_at(keys[k], phf.positions[p]);
            p += 1;
        }
        k += 1;
    }

    // unique symbols, most frequent first
    let mut syms = [Symbol {
        pos: 0,
        ch: 0,
        freq: 0,
    }; MAX_POSITIONS * 256];
    let mut nsyms = 0;
    let mut p = 0;
    while p < num_positions {
        let mut freq = [0; 256];
        let mut k = 0;
        while k < N {
            let c = kchars[k][p];
            if c < 256 {
                freq[c] += 1;
            }
            k += 1;
        }
        let mut c = 0;
        while c < 256 {
            if freq[c] > 0 {
                syms[nsyms] = Symbol {
                    pos: p,
                    ch: c,
                    freq: freq[c],
                };
                nsyms += 1;
            }
            c += 1;
        }
        p += 1;
    }
    let mut i = 0;
    while i < nsyms {
        let mut j = i + 1;
        while j < nsyms {
            if syms[j].freq > syms[i].freq {
                syms.swap(i, j);
            }
            j += 1;
        }
        i += 1;
    }

    // partial hash of each key: its length plus the determined association values
    let mut phash = [0; N];
    let mut k = 0;
    while k < N {
        phash[k] = keys[k].len();
        k += 1;
    }

    // per-symbol salts; a key's signature XORs the salts of its undetermined symbols
    let mut salt = [[0u64; 256]; MAX_POSITIONS];
    let mut s = 0x9e37_79b9_7f4a_7c15u64;
    let mut p = 0;
    while p < num_positions {
        let mut c = 0;
        while c < 256 {
            s = s
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            salt[p][c] = s;
            c += 1;
        }
        p += 1;
    }
    let mut sig = [0u64; N];
    let mut k = 0;
    while k < N {
        let mut s = 0;
        let mut p = 0;
        while p < num_positions {
            let c = kchars[k][p];
            if c < 256 {
                s ^= salt[p][c];
            }
            p += 1;
        }
        sig[k] = s;
        k += 1;
    }
    let mut order = [0; N];
    let mut k = 0;
    while k < N {
        order[k] = k;
        k += 1;
    }

    // a generation counter spares clearing the slot marks for every class check
    let mut slot_gen = [0usize; MAX_TABLE_SIZE];
    let mut generation = 0;
    let search_limit = if next_power_of_2(m) < 32 {
        32
    } else {
        next_power_of_2(m)
    };

    let mut si = 0;
    while si < nsyms {
        let sp = syms[si].pos;
        let sc = syms[si].ch;

        // mark (sp, sc) determined in the signature of every key using it
        let sp_salt = salt[sp][sc];
        let mut k = 0;
        while k < N {
            if kchars[k][sp] == sc {
                sig[k] ^= sp_salt;
            }
            k += 1;
        }

        // insertion sort keeps `order` sorted by signature, near-linear on the near-sorted input
        let mut i = 1;
        while i < N {
            let x = order[i];
            let xs = sig[x];
            let mut j = i;
            while j > 0 && sig[order[j - 1]] > xs {
                order[j] = order[j - 1];
                j -= 1;
            }
            order[j] = x;
            i += 1;
        }

        // first value keeping every equivalence class collision-free
        let mut found = false;
        let mut v = 0;
        while v < search_limit && !found {
            let mut collision = false;
            let mut ci = 0;
            while ci < N && !collision {
                let class_sig = sig[order[ci]];
                let mut cj = ci;
                while cj < N && sig[order[cj]] == class_sig {
                    cj += 1;
                }
                if cj - ci > 1 {
                    generation += 1;
                    let mut x = ci;
                    while x < cj {
                        let k = order[x];
                        let mut h = phash[k];
                        if kchars[k][sp] == sc {
                            h += v;
                        }
                        h %= m;
                        if slot_gen[h] == generation {
                            collision = true;
                            break;
                        }
                        slot_gen[h] = generation;
                        x += 1;
                    }
                }
                ci = cj;
            }
            if !collision {
                phf.asso_values[sp][sc] = v;
                let mut k = 0;
                while k < N {
                    if kchars[k][sp] == sc {
                        phash[k] += v;
                    }
                    k += 1;
                }
                found = true;
            }
            v += 1;
        }
        if !found {
            return None;
        }
        si += 1;
    }

    if place_keys(&mut phf, &phash) {
        Some(phf)
    } else {
        None
    }
}

/// A `(position, byte)` symbol whose association value is still to be determined
#[derive(Clone, Copy)]
struct Symbol {
    pos: usize,
    ch: usize,
    freq: usize,
}

/// Fills `slot_to_key` from each key's final hash, failing on a collision
const fn place_keys<const N: usize>(phf: &mut Phf, hashes: &[usize; N]) -> bool {
    let m = phf.table_size;
    let mut i = 0;
    while i < N {
        let slot = hashes[i] % m;
        if phf.slot_to_key[slot] != N {
            return false;
        }
        phf.slot_to_key[slot] = i;
        i += 1;
    }
    true
}

/// Hash-and-Displace: buckets on first/last byte and length, displaced over a 2-byte, then 4-byte, key hash
const fn try_hash_and_displace<const N: usize>(keys: &[&[u8]; N], m: usize) -> Option<Phf> {
    let mut phf = Phf::empty(N, m);
    if positions_distinguish(keys, &[], 0, m) {
        let mut lengths = [0; N];
        let mut i = 0;
        while i < N {
            lengths[i] = keys[i].len();
            i += 1;
        }
        return if place_keys(&mut phf, &lengths) {
            Some(phf)
        } else {
            None
        };
    }

    phf.num_positions = HD_MODE;
    let mut key_bucket = [0; N];
    let mut hash_2 = [0; N];
    let mut hash_4 = [0; N];
    let mut i = 0;
    while i < N {
        key_bucket[i] = hd_bucket_hash(keys[i]);
        hash_2[i] = hd_key_hash_2(keys[i]);
        hash_4[i] = hd_key_hash_4(keys[i]);
        i += 1;
    }
    let max_displacement = if m < 255 { m } else { 255 };
    if place_buckets(&mut phf, &key_bucket, &hash_2, max_displacement) {
        phf.positions[0] = HD_HASH_2BYTE_FLAG;
        return Some(phf);
    }
    if place_buckets(&mut phf, &key_bucket, &hash_4, max_displacement) {
        phf.positions[0] = HD_HASH_4BYTE_FLAG;
        return Some(phf);
    }
    None
}

/// Bucket of the Hash-and-Displace tier: first byte, last byte and length
pub(crate) const fn hd_bucket_hash(key: &[u8]) -> usize {
    let (c0, c1) = match (key.first(), key.last()) {
        (Some(&first), Some(&last)) => (first as usize, last as usize),
        _ => (0, 0),
    };
    (c0 + c1 * 3 + key.len() * 17) & 0xFF
}

/// Hash-and-Displace key hash over the length and the first 2 bytes
pub(crate) const fn hd_key_hash_2(key: &[u8]) -> usize {
    let mut kc = key.len();
    kc = kc.wrapping_mul(31).wrapping_add(safe_byte(key, 0) as usize);
    kc = kc.wrapping_mul(31).wrapping_add(safe_byte(key, 1) as usize);
    kc
}

/// Hash-and-Displace key hash over the length and the first 4 bytes
pub(crate) const fn hd_key_hash_4(key: &[u8]) -> usize {
    let mut kc = hd_key_hash_2(key);
    kc = kc.wrapping_mul(31).wrapping_add(safe_byte(key, 2) as usize);
    kc = kc.wrapping_mul(31).wrapping_add(safe_byte(key, 3) as usize);
    kc
}

/// Byte `idx` of the key, or 0 past its end: always loads, from byte 0 when out of range, so it never branches
#[inline(always)]
pub(crate) const fn safe_byte(key: &[u8], idx: usize) -> u8 {
    const ZERO: &[u8; 1] = &[0];
    let has = idx < key.len();
    let mask = (has as usize).wrapping_neg();
    let base = if key.is_empty() {
        ZERO.as_ptr()
    } else {
        key.as_ptr()
    };
    // SAFETY: the index is `idx < len` or 0, and `base` points to at least one byte
    let byte = unsafe { *base.add(idx & mask) };
    byte & (has as u8).wrapping_neg()
}

/// Places buckets largest first at the smallest displacement landing all their keys on free slots
const fn place_buckets<const N: usize>(
    phf: &mut Phf,
    key_bucket: &[usize; N],
    key_hash: &[usize; N],
    max_displacement: usize,
) -> bool {
    let m = phf.table_size;
    let (ids, _, num_buckets) = sorted_buckets(key_bucket);
    phf.slot_to_key = [N; MAX_TABLE_SIZE];
    phf.asso_values[0] = [0; 256];
    let mut b = 0;
    while b < num_buckets {
        let id = ids[b];
        let mut bucket_keys = [0; N];
        let mut bucket_len = 0;
        let mut i = 0;
        while i < N {
            if key_bucket[i] == id {
                bucket_keys[bucket_len] = i;
                bucket_len += 1;
            }
            i += 1;
        }

        let mut placed = false;
        let mut d = 0;
        while d < max_displacement && !placed {
            let mut ok = true;
            let mut bucket_slots = [0; N];
            let mut k = 0;
            while k < bucket_len {
                let slot = d.wrapping_add(key_hash[bucket_keys[k]]) % m;
                if phf.slot_to_key[slot] != N {
                    ok = false;
                    break;
                }
                let mut k2 = 0;
                while k2 < k {
                    if bucket_slots[k2] == slot {
                        ok = false;
                        break;
                    }
                    k2 += 1;
                }
                if !ok {
                    break;
                }
                bucket_slots[k] = slot;
                k += 1;
            }
            if ok {
                phf.asso_values[0][id] = d;
                let mut k = 0;
                while k < bucket_len {
                    phf.slot_to_key[bucket_slots[k]] = bucket_keys[k];
                    k += 1;
                }
                placed = true;
            }
            d += 1;
        }
        if !placed {
            return false;
        }
        b += 1;
    }
    true
}

/// Distinct buckets of the keys, largest first, with their sizes
const fn sorted_buckets<const N: usize>(
    key_bucket: &[usize; N],
) -> ([usize; N], [usize; N], usize) {
    let mut ids = [0; N];
    let mut counts = [0; N];
    let mut num_buckets = 0;
    let mut i = 0;
    while i < N {
        let mut found = false;
        let mut b = 0;
        while b < num_buckets {
            if ids[b] == key_bucket[i] {
                counts[b] += 1;
                found = true;
                break;
            }
            b += 1;
        }
        if !found {
            ids[num_buckets] = key_bucket[i];
            counts[num_buckets] = 1;
            num_buckets += 1;
        }
        i += 1;
    }
    let mut i = 0;
    while i < num_buckets {
        let mut j = i + 1;
        while j < num_buckets {
            if counts[j] > counts[i] {
                ids.swap(i, j);
                counts.swap(i, j);
            }
            j += 1;
        }
        i += 1;
    }
    (ids, counts, num_buckets)
}

/// Seeded whole-key hash plus displacement, re-rolling the seed past structural collisions
const fn try_seeded_fullhash<const N: usize>(keys: &[&[u8]; N], m: usize) -> Option<Phf> {
    let mut phf = Phf::empty(N, m);
    let max_displacement = if m < 256 { m } else { 256 };
    let mut seed = 0;
    while seed < MAX_SEEDS {
        let mut key_hash = [0; N];
        let mut key_bucket = [0; N];
        let mut i = 0;
        while i < N {
            let h = fh_hash(keys[i], seed);
            key_hash[i] = h as usize;
            key_bucket[i] = ((h >> 48) & 0xFF) as usize;
            i += 1;
        }

        // two same-bucket keys congruent mod m can't be separated by any displacement
        let mut dead = false;
        let mut i = 0;
        while i < N && !dead {
            let mut j = i + 1;
            while j < N {
                if key_bucket[i] == key_bucket[j] && key_hash[i] % m == key_hash[j] % m {
                    dead = true;
                    break;
                }
                j += 1;
            }
            i += 1;
        }

        if !dead && place_buckets(&mut phf, &key_bucket, &key_hash, max_displacement) {
            phf.num_positions = FULLHASH_MODE;
            phf.seed = seed;
            return Some(phf);
        }
        seed += 1;
    }
    None
}

/// Seeded hash folding every 8-byte chunk of the key, finished with the murmur3 avalanche
pub(crate) const fn fh_hash(key: &[u8], seed: u64) -> u64 {
    let len = key.len() as u64;
    let mut h = seed ^ FH_MUL.wrapping_mul(len + 1);
    let mut off = 0;
    while off < key.len() {
        let chunk = fh_load_chunk(key, off);
        h = (h ^ chunk).wrapping_mul(FH_MUL);
        h ^= h >> 32;
        off += 8;
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    h
}

/// Little-endian 8-byte chunk of the key at `off`, zero-padded past its end
const fn fh_load_chunk(key: &[u8], off: usize) -> u64 {
    let (_, rest) = key.split_at(off);
    if let Some(chunk) = rest.first_chunk::<8>() {
        return u64::from_le_bytes(*chunk);
    }
    let mut chunk = 0;
    let mut b = 0;
    while b < rest.len() {
        chunk |= (rest[b] as u64) << (8 * b);
        b += 1;
    }
    chunk
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;

    /// Hash family selected by the generator
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Algorithm {
        /// `len + Σ asso[i][key[pos_i]]`, gperf-style association values over a few byte positions
        Gperf,
        /// Displacement of a first/last/length bucket plus a hash of the first 2 or 4 bytes
        HashAndDisplace,
        /// Displacement over a seeded hash of every byte of the key
        SeededFullHash,
    }

    impl Algorithm {
        /// Family encoded by a `num_positions` value
        pub(crate) const fn from_num_positions(num_positions: usize) -> Self {
            match num_positions {
                HD_MODE => Self::HashAndDisplace,
                FULLHASH_MODE => Self::SeededFullHash,
                _ => Self::Gperf,
            }
        }
    }

    impl Phf {
        /// Runs only the gperf tier at exactly `table_size` slots, which need not be a power of two
        pub(crate) const fn gperf<const N: usize>(keys: &[&[u8]; N], table_size: usize) -> Self {
            assert!(
                table_size >= N && table_size <= MAX_TABLE_SIZE,
                "table size out of range"
            );
            match try_generate_gperf(keys, table_size) {
                Some(phf) => phf,
                None => panic!("failed to find asso_values for perfect hash"),
            }
        }

        /// Runs only the Hash-and-Displace tier at exactly `table_size` slots
        pub(crate) const fn hash_and_displace<const N: usize>(
            keys: &[&[u8]; N],
            table_size: usize,
        ) -> Self {
            assert!(
                table_size >= N && table_size <= MAX_TABLE_SIZE,
                "table size out of range"
            );
            match try_hash_and_displace(keys, table_size) {
                Some(phf) => phf,
                None => panic!("Hash-and-Displace failed at this table size"),
            }
        }

        /// Hash family the generator settled on
        pub(crate) const fn algorithm(&self) -> Algorithm {
            Algorithm::from_num_positions(self.num_positions)
        }
    }
}
