// Port of ConstexprCore/perfect_hash by Daniel Lemire and Francisco Thiesen, MIT

//! NEON key comparisons over page-safe 16-byte loads that may read past either end of the key

use core::arch::aarch64::{
    uint8x16_t, vaddq_u8, vandq_u8, vceqq_u8, vdupq_n_u8, vget_lane_u64, vgetq_lane_u64, vld1q_u8,
    vorrq_u8, vqtbl1q_u8, vreinterpret_u64_u8, vreinterpretq_u16_u8, vreinterpretq_u64_u8,
    vshrn_n_u16,
};
use core::arch::asm;

/// Smallest page size any target uses: a 16-byte load inside one such block can't fault
const BLOCK: usize = 4096;
/// Offset into a block past which a 16-byte load would cross into the next block
const LAST_SAFE_OFFSET: usize = BLOCK - 16;
/// Offset added to the key length to index `SLIDING_MASKS` for chunk 0 of the longest keys
const SLIDING_BASE: usize = 16 * 15;

/// TBL index rows: row `len` keeps bytes `0..len` and zeroes the rest (TBL maps 0x80 to 0)
static TBL_MASKS: [[u8; 16]; 17] = tbl_masks();

/// TBL rows realigning a window loaded `o` bytes early: keep `need` bytes, shift back by `o`
static REALIGN_ROWS: [[[u8; 16]; 16]; 17] = realign_rows();

/// TBL rows for chunk `i` of a key of length `len`: row `len + SLIDING_BASE - 16 * i`
static SLIDING_MASKS: [[u8; 16]; SLIDING_BASE + 255] = sliding_masks();

/// Keys of at most 8 bytes: the masked low lane against the packed stored key
#[inline(always)]
pub(crate) fn compare_8(key: &[u8], stored_packed: u64) -> bool {
    let raw = page_safe_load_16(key);
    // SAFETY: NEON is enabled at compile time, as the module cfg requires
    let input_packed = unsafe {
        let masked = vqtbl1q_u8(raw, row(&TBL_MASKS[key.len()]));
        vgetq_lane_u64::<0>(vreinterpretq_u64_u8(masked))
    };
    input_packed == stored_packed
}

/// Keys of at most 16 bytes: one masked chunk against the zero-padded stored key
#[inline(always)]
pub(crate) fn compare_16<const W: usize>(key: &[u8], stored: &[u8; W]) -> bool {
    let raw = page_safe_load_16(key);
    // SAFETY: NEON is enabled at compile time, as the module cfg requires
    let eq = unsafe {
        let input = vqtbl1q_u8(raw, row(&TBL_MASKS[key.len()]));
        vceqq_u8(input, stored_16(stored, 0))
    };
    all_equal(eq)
}

/// Keys over 16 bytes: `W / 16` chunk compares behind one block guard, a key near a block end copied out first
#[inline(always)]
pub(crate) fn compare_chunks<const W: usize>(key: &[u8], stored: &[u8; W]) -> bool {
    let len = key.len();
    if W == 32 {
        return compare_2chunks(key, stored);
    }
    let p = key.as_ptr();
    let off = p.addr() & (BLOCK - 1);
    if off <= BLOCK - W {
        return compare_chunks_direct(p, len, stored);
    }
    let mut buf = [0u8; W];
    buf[..len].copy_from_slice(key);
    compare_chunks_direct(buf.as_ptr(), len, stored)
}

/// 16 bytes, the first `len` the key's: near a block end, the block's last 16 bytes shifted down by one TBL
#[inline(always)]
pub(crate) fn page_safe_load_16(key: &[u8]) -> uint8x16_t {
    let len = key.len();
    debug_assert!(len <= 16);
    let p = key.as_ptr();
    let off = p.addr() & (BLOCK - 1);
    if off <= LAST_SAFE_OFFSET {
        // SAFETY: the 16 bytes stay inside the block holding the key's first byte, which is mapped
        return unsafe { load_mapped_16(p) };
    }
    let o = if off > BLOCK - len {
        0
    } else {
        off - LAST_SAFE_OFFSET
    };
    // SAFETY: with `o > 0` the window ends at the block end; with `o == 0` the key reaches the next block
    let raw = unsafe { load_mapped_16(p.wrapping_sub(o)) };
    // SAFETY: NEON is enabled at compile time, as the module cfg requires
    unsafe {
        let shift = vaddq_u8(row(&TBL_MASKS[16]), vdupq_n_u8(o as u8));
        vqtbl1q_u8(raw, shift)
    }
}

/// Keys of 17 to 32 bytes, branch-free: a first chunk realigned off the block end, plus the key's last 16 bytes
#[inline(always)]
fn compare_2chunks<const W: usize>(key: &[u8], stored: &[u8; W]) -> bool {
    let len = key.len();
    let p = key.as_ptr();
    let off = p.addr() & (BLOCK - 1);
    let need = if len < 16 { len } else { 16 };
    let early = off.saturating_sub(LAST_SAFE_OFFSET);
    let o = if off > BLOCK - need { 0 } else { early };
    let p1 = p.wrapping_sub(o);
    // SAFETY: the window ends at or before the block end, or the key reaches into the next block
    let window1 = unsafe { load_mapped_16(p1) };
    // SAFETY: NEON is enabled at compile time, as the module cfg requires
    let eq1 = unsafe {
        let chunk1 = vqtbl1q_u8(window1, row(&REALIGN_ROWS[need][o]));
        vceqq_u8(chunk1, stored_16(stored, 0))
    };

    let short_key = len < 16;
    let off2 = if short_key { 0 } else { len - 16 };
    let p2 = if short_key { p1 } else { p.wrapping_add(off2) };
    // SAFETY: `p2` is the first window or the key's last 16 bytes
    let window2 = unsafe { load_mapped_16(p2) };
    // SAFETY: NEON is enabled at compile time, as the module cfg requires
    let eq = unsafe {
        let forced = vdupq_n_u8((short_key as u8).wrapping_neg());
        let eq2 = vorrq_u8(vceqq_u8(window2, stored_16(stored, off2)), forced);
        vandq_u8(eq1, eq2)
    };
    all_equal(eq)
}

/// All `W / 16` chunks with sliding masks, the caller guaranteeing `W` readable bytes at `p`
#[inline(always)]
fn compare_chunks_direct<const W: usize>(p: *const u8, len: usize, stored: &[u8; W]) -> bool {
    let chunks = W / 16;
    // SAFETY: NEON is enabled at compile time, as the module cfg requires
    let mut eq = unsafe { vdupq_n_u8(0xFF) };
    let mut i = 0;
    while i < chunks {
        // SAFETY: the caller guarantees `[p, p + W)` is mapped
        let window = unsafe { load_mapped_16(p.wrapping_add(16 * i)) };
        let mask = row(&SLIDING_MASKS[len + SLIDING_BASE - 16 * i]);
        // SAFETY: NEON is enabled at compile time, as the module cfg requires
        eq = unsafe {
            let chunk = vqtbl1q_u8(window, mask);
            vandq_u8(eq, vceqq_u8(chunk, stored_16(stored, 16 * i)))
        };
        i += 1;
    }
    all_equal(eq)
}

/// Loads 16 mapped bytes at `p`, which may lie outside any Rust allocation: opaque asm, so no Rust out-of-bounds read
#[inline(always)]
unsafe fn load_mapped_16(p: *const u8) -> uint8x16_t {
    let v: uint8x16_t;
    // SAFETY: the caller guarantees the 16 bytes are mapped, and the asm only reads them
    unsafe {
        asm!(
            "ldr {v:q}, [{p}]",
            p = in(reg) p,
            v = out(vreg) v,
            options(pure, readonly, nostack, preserves_flags),
        );
    }
    v
}

/// TBL index row `row`, a static row of 16 indices
#[inline(always)]
fn row(row: &[u8; 16]) -> uint8x16_t {
    // SAFETY: `row` holds 16 readable bytes
    unsafe { vld1q_u8(row.as_ptr()) }
}

/// 16 stored key bytes at `offset`, within the zero-padded stored key
#[inline(always)]
fn stored_16<const W: usize>(stored: &[u8; W], offset: usize) -> uint8x16_t {
    debug_assert!(offset + 16 <= W);
    // SAFETY: callers pass `offset + 16 <= W`, the stored key being padded to whole chunks
    unsafe { vld1q_u8(stored.as_ptr().add(offset)) }
}

/// Whether all 16 lanes of an equality mask are set, the narrowing shift pairing adjacent bytes
#[inline(always)]
fn all_equal(mask: uint8x16_t) -> bool {
    // SAFETY: NEON is enabled at compile time, as the module cfg requires
    let narrowed = unsafe { vshrn_n_u16::<4>(vreinterpretq_u16_u8(mask)) };
    // SAFETY: NEON is enabled at compile time, as the module cfg requires
    let lanes = unsafe { vget_lane_u64::<0>(vreinterpret_u64_u8(narrowed)) };
    lanes == u64::MAX
}

const fn tbl_masks() -> [[u8; 16]; 17] {
    let mut rows = [[0x80; 16]; 17];
    let mut len = 0;
    while len <= 16 {
        let mut b = 0;
        while b < len {
            rows[len][b] = b as u8;
            b += 1;
        }
        len += 1;
    }
    rows
}

const fn realign_rows() -> [[[u8; 16]; 16]; 17] {
    let mut rows = [[[0x80; 16]; 16]; 17];
    let mut need = 0;
    while need <= 16 {
        let mut o = 0;
        while o < 16 {
            let mut j = 0;
            while j < 16 {
                if j < need && j + o < 16 {
                    rows[need][o][j] = (j + o) as u8;
                }
                j += 1;
            }
            o += 1;
        }
        need += 1;
    }
    rows
}

const fn sliding_masks() -> [[u8; 16]; SLIDING_BASE + 255] {
    let mut rows = [[0x80; 16]; SLIDING_BASE + 255];
    let mut v = SLIDING_BASE;
    while v < SLIDING_BASE + 255 {
        let kept = v - SLIDING_BASE;
        let keep = if kept > 16 { 16 } else { kept };
        let mut b = 0;
        while b < keep {
            rows[v][b] = b as u8;
            b += 1;
        }
        v += 1;
    }
    rows
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;

    /// First `len` key bytes zero-padded to 16, as two little-endian lanes
    pub(crate) fn load_chunk_16(key: &[u8]) -> (u64, u64) {
        let raw = page_safe_load_16(key);
        // SAFETY: NEON is enabled at compile time, as the module cfg requires
        unsafe {
            let chunk = vqtbl1q_u8(raw, row(&TBL_MASKS[key.len()]));
            let lanes = vreinterpretq_u64_u8(chunk);
            (vgetq_lane_u64::<0>(lanes), vgetq_lane_u64::<1>(lanes))
        }
    }
}
