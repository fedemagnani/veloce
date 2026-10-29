// Port of ConstexprCore/perfect_hash by Daniel Lemire and Francisco Thiesen, MIT

//! Perfect hash sets and maps of static keys, computed entirely at compile time
//!
//! A Rust port of the classic container of [ConstexprCore/perfect_hash] v0.4.0 by Daniel Lemire and
//! Francisco Thiesen, MIT licensed.
//!
//! ## Example
//!
//! ```rust
//! use veloce::{perfect_map, perfect_set};
//!
//! perfect_set!(static METHODS = ["GET", "POST", "PUT", "DELETE", "PATCH"];);
//!
//! assert!(METHODS.contains("GET"));
//! assert!(!METHODS.contains("get"));
//! assert_eq!(METHODS.index_of("PUT"), Some(2));
//!
//! #[derive(Debug, PartialEq)]
//! enum Method { Get, Post }
//!
//! perfect_map!(static PARSE: Method = { "GET" => Method::Get, "POST" => Method::Post };);
//!
//! assert_eq!(PARSE.get("POST"), Some(&Method::Post));
//! assert_eq!(PARSE.get(b"HEAD"), None);
//! ```
//!
//! ## Limits
//!
//! - 1 to 255 distinct keys, each a `&str` constant of at most 254 bytes, the empty key included
//! - lookups take any `AsRef<[u8]>`: `&str`, `&[u8]`, `String`, byte arrays
//! - a set no tier can hash into at most 256 slots fails to compile, as does a duplicate key
//! - the generator runs in const evaluation, so a large or adversarial set lengthens compile times
//!
//! ## How it works
//!
//! One hash family serves the whole set, picked by fallback rather than by any score: the first that fits wins:
//! 1. gperf-style, `len + Σ asso[i][key[pos_i]]`: fails when no 16 byte positions separate same-length keys
//! 2. Hash-and-Displace, a first/last/length bucket plus a 2- or 4-byte hash: fails when same-bucket keys collide
//! 3. seeded whole-key hash, re-rolling up to 512 seeds: the last resort, failing it fails the build
//!
//!
//! ## Type parameters
//!
//! [`PerfectSet<N, T, L, W>`] and [`PerfectMap<V, N, T, L, W>`] take the key count `N`, the table size `T`, the
//! longest key length `L` and the stored key width `W`, `L` rounded up to 16: the macros compute them.
//!
//! [ConstexprCore/perfect_hash]: https://github.com/ConstexprCore/perfect_hash

#[cfg(codegen)]
mod codegen;
mod generator;
mod map;
#[cfg(all(target_arch = "aarch64", target_feature = "neon", not(miri)))]
mod neon;
mod set;
#[cfg(test)]
mod test;

pub use generator::{Phf, padded_len};
pub use map::PerfectMap;
pub use set::PerfectSet;

#[doc(hidden)]
pub use paste::paste as __paste;

/// Defines a `static` [`PerfectSet`] of `&str` keys, the perfect hash computed at compile time
///
/// ```rust
/// veloce::perfect_set!(pub static PROTOCOLS = ["http", "https", "ftp", "ws", "wss", "file"];);
///
/// assert!(PROTOCOLS.contains("wss"));
/// ```
#[macro_export]
macro_rules! perfect_set {
    ($(#[$attr:meta])* $vis:vis static $name:ident = [$($key:expr),+ $(,)?];) => {
        $crate::perfect_hash::__paste! {
            #[doc(hidden)]
            #[allow(non_upper_case_globals)]
            const [<__PERFECT_HASH_ $name _KEYS>]: [&[u8]; [$($key),+].len()] = [$(str::as_bytes($key)),+];
            #[doc(hidden)]
            #[allow(long_running_const_eval, non_upper_case_globals)]
            const [<__PERFECT_HASH_ $name _PHF>]: $crate::perfect_hash::Phf =
                $crate::perfect_hash::Phf::new(&[<__PERFECT_HASH_ $name _KEYS>]);
            $(#[$attr])*
            #[allow(long_running_const_eval)]
            $vis static $name: $crate::perfect_hash::PerfectSet<
                { [<__PERFECT_HASH_ $name _KEYS>].len() },
                { [<__PERFECT_HASH_ $name _PHF>].table_size() },
                { $crate::perfect_hash::key_len_bound(&[<__PERFECT_HASH_ $name _KEYS>]) },
                { $crate::perfect_hash::padded_len($crate::perfect_hash::key_len_bound(&[<__PERFECT_HASH_ $name _KEYS>])) },
            > = $crate::perfect_hash::PerfectSet::new(
                &[<__PERFECT_HASH_ $name _KEYS>],
                &[<__PERFECT_HASH_ $name _PHF>],
            );
        }
    };
}

/// Defines a `static` [`PerfectMap`] from `&str` keys to values of a given type, hashed at compile time
///
/// ```rust
/// veloce::perfect_map!(pub static STATUS: u16 = { "OK" => 200, "Not Found" => 404 };);
///
/// assert_eq!(STATUS.get("Not Found"), Some(&404));
/// ```
#[macro_export]
macro_rules! perfect_map {
    ($(#[$attr:meta])* $vis:vis static $name:ident: $value:ty = { $($key:expr => $val:expr),+ $(,)? };) => {
        $crate::perfect_hash::__paste! {
            #[doc(hidden)]
            #[allow(non_upper_case_globals)]
            const [<__PERFECT_HASH_ $name _KEYS>]: [&[u8]; [$($key),+].len()] = [$(str::as_bytes($key)),+];
            #[doc(hidden)]
            #[allow(long_running_const_eval, non_upper_case_globals)]
            const [<__PERFECT_HASH_ $name _PHF>]: $crate::perfect_hash::Phf =
                $crate::perfect_hash::Phf::new(&[<__PERFECT_HASH_ $name _KEYS>]);
            $(#[$attr])*
            #[allow(long_running_const_eval)]
            $vis static $name: $crate::perfect_hash::PerfectMap<
                $value,
                { [<__PERFECT_HASH_ $name _KEYS>].len() },
                { [<__PERFECT_HASH_ $name _PHF>].table_size() },
                { $crate::perfect_hash::key_len_bound(&[<__PERFECT_HASH_ $name _KEYS>]) },
                { $crate::perfect_hash::padded_len($crate::perfect_hash::key_len_bound(&[<__PERFECT_HASH_ $name _KEYS>])) },
            > = $crate::perfect_hash::PerfectMap::new(
                &[<__PERFECT_HASH_ $name _KEYS>],
                [$($val),+],
                &[<__PERFECT_HASH_ $name _PHF>],
            );
        }
    };
}

/// Longest key length, at least 1: the `L` parameter of a set or map of these keys
pub const fn key_len_bound<const N: usize>(keys: &[&[u8]; N]) -> usize {
    let max = generator::max_key_len(keys);
    if max == 0 { 1 } else { max }
}
