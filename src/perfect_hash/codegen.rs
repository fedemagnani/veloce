//! Probes of the perfect_hash lookups for `cargo codegen`, one `get` per bench key set so every compare path has one

// the key sets of the benches, so the probes cover exactly what the benches measure
include!("keysets.rs");

/// One probe block with a map and a `get` probe per key set, then [`header_probes`] on every set
macro_rules! probe_keysets {
    ($($tier:ident $name:ident { $($hit:literal => $value:literal),+ $(,)? } misses $misses:tt)*) => {
        $crate::perfect_hash::__paste! {
            ::codegen::probes!(perfect_hash_probes {
                // the streams of the perfect_hash criterion benches, for `cargo codegen measure`
                mod streams {
                    /// Lookups per stream, as the benches: long enough that the predictor can't memorize it
                    const STREAM_LEN: usize = 2_000_000;
                    /// Block no key straddles, as the benches: a key crossing a page costs ~10-20 ns per lookup
                    const BLOCK: usize = 4096;

                    /// `STREAM_LEN` keys drawn uniformly from `pool`, copied into a leaked block-aligned arena
                    pub fn stream(rng: &mut ::codegen::Rng, pool: &[&str]) -> Vec<&'static str> {
                        let total: usize = pool.iter().map(|key| key.len() + BLOCK).sum();
                        let layout = ::std::alloc::Layout::from_size_align(total, BLOCK).expect("arena layout");
                        // SAFETY: the layout has a nonzero size, the arena is leaked so its keys are `'static`
                        let base = unsafe { ::std::alloc::alloc_zeroed(layout) };
                        assert!(!base.is_null(), "arena allocation failed");
                        // SAFETY: `base` points to `total` zeroed bytes owned by nobody else
                        let arena: &'static mut [u8] = unsafe { ::std::slice::from_raw_parts_mut(base, total) };
                        let mut ranges = Vec::with_capacity(pool.len());
                        let mut offset = 0;
                        for key in pool {
                            let block_end = (offset / BLOCK + 1) * BLOCK;
                            if offset + key.len() > block_end {
                                offset = block_end;
                            }
                            arena[offset..offset + key.len()].copy_from_slice(key.as_bytes());
                            ranges.push(offset..offset + key.len());
                            offset += key.len();
                        }
                        let arena: &'static [u8] = arena;
                        let interned: Vec<&'static str> = ranges
                            .into_iter()
                            .map(|range| ::std::str::from_utf8(&arena[range]).expect("utf-8 key"))
                            .collect();
                        rng.indices(STREAM_LEN, interned.len()).map(|i| interned[i]).collect()
                    }
                }

                $(
                    $crate::perfect_map!(static [<$name:upper>]: i32 = { $($hit => $value),+ };);

                    fn [<perfect_hash_get_ $name>](key: &str) -> Option<i32> {
                        [<$name:upper>].get(key).copied()
                    }

                    #[codegen::inputs([<perfect_hash_get_ $name>])]
                    fn [<perfect_hash_get_ $name _inputs>](m: &mut codegen::Measure) {
                        let hits: &[&str] = &[$($hit),+];
                        let misses: &[&str] = &$misses;
                        let mixed: Vec<&str> = hits.iter().chain(misses).copied().collect();
                        for (case, pool) in [("hits", hits), ("misses", misses), ("mixed", &mixed[..])] {
                            let stream = streams::stream(m.rng(), pool);
                            m.case(case).run(&stream, |&key| [<perfect_hash_get_ $name>](key));
                        }
                    }
                )*
            });
        }
        $(header_probes!($name { $($hit => $value),+ });)*
    };
}

/// A probe block with `contains` and `index_of` on the `header` set, nothing for the other sets
macro_rules! header_probes {
    (header { $($hit:literal => $value:literal),+ }) => {
        ::codegen::probes!(perfect_hash_header_probes {
            $crate::perfect_map!(static HEADER: i32 = { $($hit => $value),+ };);
            // `index_of` is only on sets, so it gets a set of the same keys
            $crate::perfect_set!(static HEADER_SET = [$($hit),+];);

            fn perfect_hash_contains_header(key: &str) -> bool {
                HEADER.contains_key(key)
            }

            fn perfect_hash_index_of_header(key: &str) -> Option<usize> {
                HEADER_SET.index_of(key)
            }
        });
    };
    ($other:ident $hits:tt) => {};
}

// after the macros it expands to, `macro_rules!` being scoped textually
keysets!(probe_keysets);
