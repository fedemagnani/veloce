//! Query streams of the perfect_hash libtest and criterion benches, as upstream builds them, only longer

/// Queries per stream, ten times upstream's 200 000: the branch predictor memorizes a stream replayed that short,
/// hiding up to a fifth of a mixed stream's mispredicts
const STREAM_LEN: usize = 2_000_000;
/// Seed of every stream, as upstream
const SEED: u64 = 42;

/// Block no query key straddles: a key crossing a page costs ~10-20 ns per lookup, so placement would decide results
const BLOCK: usize = 4096;

/// Lookups of a stream, drawn from the keys, the non-keys, or both pools concatenated
#[derive(Clone, Copy)]
pub enum Mix {
    Hits,
    Misses,
    Mixed,
}

/// Pre-shuffled query stream of a mix, `|hits| / (|hits| + |misses|)` hits when mixed
pub fn stream(mix: Mix, hits: &[&'static str], misses: &[&'static str]) -> Vec<&'static str> {
    let pool: Vec<&'static str> = match mix {
        Mix::Hits => hits.to_vec(),
        Mix::Misses => misses.to_vec(),
        Mix::Mixed => hits.iter().chain(misses).copied().collect(),
    };
    let interned = intern(&pool);
    sample(&interned)
}

/// Copies the keys back to back into a leaked block-aligned arena, moving a key to the next block rather than straddle
fn intern(keys: &[&'static str]) -> Vec<&'static str> {
    let total: usize = keys.iter().map(|key| key.len() + BLOCK).sum();
    let layout = std::alloc::Layout::from_size_align(total, BLOCK).expect("arena layout");
    // SAFETY: the layout has a nonzero size, the arena is leaked so its keys are `'static`
    let base = unsafe { std::alloc::alloc_zeroed(layout) };
    assert!(!base.is_null(), "arena allocation failed");
    // SAFETY: `base` points to `total` zeroed bytes owned by nobody else
    let arena: &'static mut [u8] = unsafe { std::slice::from_raw_parts_mut(base, total) };
    let mut ranges = Vec::with_capacity(keys.len());
    let mut offset = 0;
    for key in keys {
        let block_end = (offset / BLOCK + 1) * BLOCK;
        if offset + key.len() > block_end {
            offset = block_end;
        }
        arena[offset..offset + key.len()].copy_from_slice(key.as_bytes());
        ranges.push(offset..offset + key.len());
        offset += key.len();
    }
    let arena: &'static [u8] = arena;
    ranges
        .into_iter()
        .map(|range| std::str::from_utf8(&arena[range]).expect("utf-8 key"))
        .collect()
}

/// `STREAM_LEN` keys drawn uniformly from `pool`
fn sample(pool: &[&'static str]) -> Vec<&'static str> {
    let mut rng = Mt19937_64::new(SEED);
    (0..STREAM_LEN)
        .map(|_| {
            let index = rng.next() % pool.len() as u64;
            pool[index as usize]
        })
        .collect()
}

/// `std::mt19937_64`, so the streams are those upstream samples
struct Mt19937_64 {
    state: [u64; 312],
    index: usize,
}

impl Mt19937_64 {
    fn new(seed: u64) -> Self {
        let mut state = [0; 312];
        state[0] = seed;
        for i in 1..312 {
            let prev = state[i - 1];
            state[i] = 6_364_136_223_846_793_005u64
                .wrapping_mul(prev ^ (prev >> 62))
                .wrapping_add(i as u64);
        }
        Self { state, index: 312 }
    }

    fn twist(&mut self) {
        const UPPER: u64 = 0xFFFF_FFFF_8000_0000;
        const LOWER: u64 = 0x7FFF_FFFF;
        for i in 0..312 {
            let x = (self.state[i] & UPPER) | (self.state[(i + 1) % 312] & LOWER);
            let mut xa = x >> 1;
            if x & 1 != 0 {
                xa ^= 0xB502_6F5A_A966_19E9;
            }
            self.state[i] = self.state[(i + 156) % 312] ^ xa;
        }
        self.index = 0;
    }

    fn next(&mut self) -> u64 {
        if self.index >= 312 {
            self.twist();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= (y >> 29) & 0x5555_5555_5555_5555;
        y ^= (y << 17) & 0x71D6_7FFF_EDA6_0000;
        y ^= (y << 37) & 0xFFF7_EEE0_0000_0000;
        y ^= y >> 43;
        y
    }
}
