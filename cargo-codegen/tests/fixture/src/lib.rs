//! Fixture of cargo-codegen's tests: probes whose assembly the tests know beforehand

/// Element `i`, its bounds check left to a panic
pub fn checked(values: &[u64], i: usize) -> u64 {
    values[i]
}

/// Element `i`, or 0 past the end, without any panic
pub fn unchecked(values: &[u64], i: usize) -> u64 {
    values.get(i).copied().unwrap_or(0)
}

/// Wrapping sum of the values, watched; `#[inline]` so its probe inlines it across the crate boundary
#[codegen::watch]
#[inline]
pub fn watched_sum(values: &[u64]) -> u64 {
    values.iter().fold(0, |sum, value| sum.wrapping_add(*value))
}

/// The same body as [`watched_sum`], unwatched
#[inline]
pub fn plain_sum(values: &[u64]) -> u64 {
    values.iter().fold(0, |sum, value| sum.wrapping_add(*value))
}

#[codegen::probe]
mod probes {
    fn fixture_checked(values: &[u64], i: usize) -> u64 {
        crate::checked(values, i)
    }

    fn fixture_unchecked(values: &[u64], i: usize) -> u64 {
        crate::unchecked(values, i)
    }

    fn fixture_watched_sum(values: &[u64]) -> u64 {
        crate::watched_sum(values)
    }

    fn fixture_plain_sum(values: &[u64]) -> u64 {
        crate::plain_sum(values)
    }

    // a body of its own, so LLVM can't merge it into another probe and drop the watched function's DWARF
    fn fixture_watched_rotated(values: &[u64]) -> u64 {
        crate::watched_sum(values).rotate_left(1)
    }
}
