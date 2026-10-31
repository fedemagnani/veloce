//! Probes of the perfect_hash lookups for `cargo codegen`, one `get` per bench key set so every compare path has one

// the key sets of the benches, so the probes cover exactly what the benches measure
include!("keysets.rs");

/// One probe block with a map and a `get` probe per key set, then [`header_probes`] on every set
macro_rules! probe_keysets {
    ($($tier:ident $name:ident { $($hit:literal => $value:literal),+ $(,)? } misses $misses:tt)*) => {
        $crate::perfect_hash::__paste! {
            ::codegen::probes!(perfect_hash_probes {
                $(
                    $crate::perfect_map!(static [<$name:upper>]: i32 = { $($hit => $value),+ };);

                    fn [<perfect_hash_get_ $name>](key: &str) -> Option<i32> {
                        [<$name:upper>].get(key).copied()
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
