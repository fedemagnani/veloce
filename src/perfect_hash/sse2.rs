// Port of ConstexprCore/perfect_hash by Daniel Lemire and Francisco Thiesen, MIT

//! SSE2 key comparisons over page-guarded 16-byte loads that may read past the end of the key

use core::arch::asm;
use core::arch::x86_64::{
    __m128i, _mm_and_si128, _mm_cmpeq_epi8, _mm_cvtsi128_si64, _mm_loadu_si128, _mm_movemask_epi8,
    _mm_set1_epi8, _mm_storeu_si128,
};

/// Smallest page size any target uses: a load inside one such block can't fault
const BLOCK: usize = 4096;
/// Offset into a block past which a 16-byte load would cross into the next block
const LAST_SAFE_OFFSET: usize = BLOCK - 16;
/// Offset added to the key length to index `SLIDING_MASKS` for chunk 0 of the longest keys
const SLIDING_BASE: usize = 16 * 15;
/// `_mm_movemask_epi8` of an all-equal comparison
const ALL_EQUAL: i32 = 0xFFFF;

/// AND rows: row `len` keeps bytes `0..len` and zeroes the rest
static AND_MASKS: [[u8; 16]; 17] = and_masks();

/// AND rows for chunk `i` of a key of length `len`: row `len + SLIDING_BASE - 16 * i`
static SLIDING_MASKS: [[u8; 16]; SLIDING_BASE + 255] = sliding_masks();

/// Keys of at most 8 bytes: the masked low lane against the packed stored key
#[inline(always)]
pub(crate) fn compare_8(key: &[u8], stored_packed: u64) -> bool {
    let masked = load_masked_16(key);
    // SAFETY: SSE2 is enabled at compile time, as the module cfg requires
    let input_packed = unsafe { _mm_cvtsi128_si64(masked) } as u64;
    input_packed == stored_packed
}

/// Keys of at most 16 bytes: one masked chunk against the zero-padded stored key
#[inline(always)]
pub(crate) fn compare_16<const W: usize>(key: &[u8], stored: &[u8; W]) -> bool {
    let input = load_masked_16(key);
    let expected = stored_16(stored, 0);
    // SAFETY: SSE2 is enabled at compile time, as the module cfg requires
    let eq = unsafe { _mm_cmpeq_epi8(input, expected) };
    all_equal(eq)
}

/// Keys over 16 bytes: `W / 16` chunk compares behind one block guard, a key near a block end copied out first
#[inline(always)]
pub(crate) fn compare_chunks<const W: usize>(key: &[u8], stored: &[u8; W]) -> bool {
    let len = key.len();
    let p = key.as_ptr();
    let off = p.addr() & (BLOCK - 1);
    if off <= BLOCK - W {
        return compare_chunks_direct(p, len, stored);
    }
    let (tail, o) = block_tail::<W>(p, off);
    let copy = tail.as_ptr().cast::<u8>().wrapping_add(o);
    // a key reaching into the next block proves it mapped, so its span reads in place
    let source = if off > BLOCK - len { p } else { copy };
    compare_chunks_direct(source, len, stored)
}

/// The first `len` key bytes zero-padded to 16, near a block end copied out first
#[inline(always)]
fn load_masked_16(key: &[u8]) -> __m128i {
    let raw = page_safe_load_16(key);
    let mask = row(&AND_MASKS[key.len()]);
    // SAFETY: SSE2 is enabled at compile time, as the module cfg requires
    unsafe { _mm_and_si128(raw, mask) }
}

/// 16 bytes, the first `len` the key's: near a block end, the key copied to a zeroed buffer
#[inline(always)]
fn page_safe_load_16(key: &[u8]) -> __m128i {
    let len = key.len();
    debug_assert!(len <= 16);
    let p = key.as_ptr();
    let off = p.addr() & (BLOCK - 1);
    if off <= LAST_SAFE_OFFSET {
        // SAFETY: the 16 bytes stay inside the block holding the key's first byte, which is mapped
        return unsafe { load_mapped_16(p) };
    }
    let (tail, o) = block_tail::<16>(p, off);
    let copy = tail.as_ptr().cast::<u8>().wrapping_add(o);
    // a key reaching into the next block proves it mapped, so it reads in place
    let source = if off > BLOCK - len { p } else { copy };
    // SAFETY: `source` is the key, its next block mapped, or 16 bytes inside the 32-byte copy as `o < 16`
    unsafe { load_mapped_16(source) }
}

/// The block's last `W` bytes, zero-padded to `2 * W`, and `p`'s offset: no `memcpy`, whose call pushes registers on every lookup
#[inline(always)]
fn block_tail<const W: usize>(p: *const u8, off: usize) -> ([[u8; W]; 2], usize) {
    let tail_start = BLOCK - W;
    let base = p.wrapping_sub(off).wrapping_add(tail_start);
    let mut tail = [[0u8; W]; 2];
    let mut i = 0;
    while i < W / 16 {
        // SAFETY: the window lies in the block holding `p`, which is mapped
        let window = unsafe { load_mapped_16(base.wrapping_add(16 * i)) };
        // SAFETY: `16 * i + 16 <= W`, inside the copy's first half
        unsafe { _mm_storeu_si128(tail[0].as_mut_ptr().add(16 * i).cast(), window) };
        i += 1;
    }
    (tail, off - tail_start)
}

/// All `W / 16` chunks with sliding masks, the caller guaranteeing `W` readable bytes at `p`
#[inline(always)]
fn compare_chunks_direct<const W: usize>(p: *const u8, len: usize, stored: &[u8; W]) -> bool {
    let chunks = W / 16;
    // SAFETY: SSE2 is enabled at compile time, as the module cfg requires
    let mut eq = unsafe { _mm_set1_epi8(-1) };
    let mut i = 0;
    while i < chunks {
        // SAFETY: the caller guarantees `[p, p + W)` is mapped
        let window = unsafe { load_mapped_16(p.wrapping_add(16 * i)) };
        let mask = row(&SLIDING_MASKS[len + SLIDING_BASE - 16 * i]);
        let expected = stored_16(stored, 16 * i);
        // SAFETY: SSE2 is enabled at compile time, as the module cfg requires
        eq = unsafe {
            let chunk = _mm_and_si128(window, mask);
            _mm_and_si128(eq, _mm_cmpeq_epi8(chunk, expected))
        };
        i += 1;
    }
    all_equal(eq)
}

/// Loads 16 mapped bytes at `p`, which may lie outside any Rust allocation: opaque asm, so no Rust out-of-bounds read
#[inline(always)]
unsafe fn load_mapped_16(p: *const u8) -> __m128i {
    let v: __m128i;
    // SAFETY: the caller guarantees the 16 bytes are mapped, and the asm only reads them
    unsafe {
        asm!(
            "movdqu {v}, xmmword ptr [{p}]",
            p = in(reg) p,
            v = out(xmm_reg) v,
            options(pure, readonly, nostack, preserves_flags),
        );
    }
    v
}

/// A static row of 16 bytes
#[inline(always)]
fn row(row: &[u8; 16]) -> __m128i {
    // SAFETY: `row` holds 16 readable bytes
    unsafe { _mm_loadu_si128(row.as_ptr().cast()) }
}

/// 16 stored key bytes at `offset`, within the zero-padded stored key
#[inline(always)]
fn stored_16<const W: usize>(stored: &[u8; W], offset: usize) -> __m128i {
    debug_assert!(offset + 16 <= W);
    // SAFETY: callers pass `offset + 16 <= W`, the stored key being padded to whole chunks
    unsafe { _mm_loadu_si128(stored.as_ptr().add(offset).cast()) }
}

/// Whether all 16 lanes of an equality mask are set
#[inline(always)]
fn all_equal(mask: __m128i) -> bool {
    // SAFETY: SSE2 is enabled at compile time, as the module cfg requires
    let bits = unsafe { _mm_movemask_epi8(mask) };
    bits == ALL_EQUAL
}

const fn and_masks() -> [[u8; 16]; 17] {
    let mut rows = [[0; 16]; 17];
    let mut len = 0;
    while len <= 16 {
        let mut b = 0;
        while b < len {
            rows[len][b] = 0xFF;
            b += 1;
        }
        len += 1;
    }
    rows
}

const fn sliding_masks() -> [[u8; 16]; SLIDING_BASE + 255] {
    let mut rows = [[0; 16]; SLIDING_BASE + 255];
    let mut v = SLIDING_BASE;
    while v < SLIDING_BASE + 255 {
        let kept = v - SLIDING_BASE;
        let keep = if kept > 16 { 16 } else { kept };
        let mut b = 0;
        while b < keep {
            rows[v][b] = 0xFF;
            b += 1;
        }
        v += 1;
    }
    rows
}

#[cfg(test)]
pub(crate) mod test {
    use core::arch::x86_64::_mm_unpackhi_epi64;

    use super::*;

    /// First `len` key bytes zero-padded to 16, as two little-endian lanes
    pub(crate) fn load_chunk_16(key: &[u8]) -> (u64, u64) {
        let chunk = load_masked_16(key);
        // SAFETY: SSE2 is enabled at compile time, as the module cfg requires
        unsafe {
            let lo = _mm_cvtsi128_si64(chunk) as u64;
            let hi = _mm_cvtsi128_si64(_mm_unpackhi_epi64(chunk, chunk)) as u64;
            (lo, hi)
        }
    }
}
