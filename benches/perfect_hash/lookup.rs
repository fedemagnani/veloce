//! Lookups over upstream's key sets, hits-only, misses-only and mixed streams, one lookup per iteration
//!
//! `bench` runs one key set per lookup path; `bench-full` adds the sets repeating a path already covered.

use std::collections::hash_map::RandomState;

use test::Bencher;

use super::common::{Mix, index_hashbrown, index_map, run};

/// A module of benches per key set, each key mapped to its declaration index, `extra` sets only under `bench-full`
macro_rules! keyset {
    () => {};
    (core $name:ident $hits:tt misses $misses:tt $($rest:tt)*) => {
        keyset!(@module $name $hits misses $misses);
        keyset!($($rest)*);
    };
    (extra $name:ident $hits:tt misses $misses:tt $($rest:tt)*) => {
        #[cfg(feature = "bench-full")]
        keyset!(@module $name $hits misses $misses);
        keyset!($($rest)*);
    };
    (@module $name:ident { $($hit:literal => $value:literal),+ $(,)? } misses [$($miss:literal),+ $(,)?]) => {
        mod $name {
            use super::*;

            const HITS: &[&str] = &[$($hit),+];
            const MISSES: &[&str] = &[$($miss),+];

            veloce::perfect_map!(static VELOCE: i32 = { $($hit => $value),+ };);
            static PHF: phf::Map<&'static str, i32> = phf::phf_map! { $($hit => $value),+ };

            bench_mixes!(hits: Hits, misses: Misses, mixed: Mixed);
        }
    };
}

/// One bench per method for each mix
macro_rules! bench_mixes {
    ($($mix:ident: $variant:ident),+) => {
        paste::paste! {
            $(
                #[bench]
                fn [<$mix _veloce>](b: &mut Bencher) {
                    run(b, Mix::$variant, HITS, MISSES, |key| VELOCE.get(key).copied());
                }

                #[bench]
                fn [<$mix _phf>](b: &mut Bencher) {
                    run(b, Mix::$variant, HITS, MISSES, |key| PHF.get(key).copied());
                }

                #[bench]
                fn [<$mix _std>](b: &mut Bencher) {
                    let map = index_map(HITS, RandomState::new());
                    run(b, Mix::$variant, HITS, MISSES, |key| map.get(key).copied());
                }

                #[bench]
                fn [<$mix _hashbrown_ahash>](b: &mut Bencher) {
                    let map = index_hashbrown(HITS, ahash::RandomState::new());
                    run(b, Mix::$variant, HITS, MISSES, |key| map.get(key).copied());
                }

                #[bench]
                fn [<$mix _hashbrown_fx>](b: &mut Bencher) {
                    let map = index_hashbrown(HITS, rustc_hash::FxBuildHasher);
                    run(b, Mix::$variant, HITS, MISSES, |key| map.get(key).copied());
                }
            )+
        }
    };
}

// after the macros it expands to, `macro_rules!` being scoped textually
keysets!(keyset);
