//! Probes of the perfect_hash lookups for the codegen tool, one `get` per bench key set so every compare path has one

// the key sets of the benches, so the probes cover exactly what the benches measure
include!("../../benches/perfect_hash/keysets.rs");

/// A map over a key set and its `get` probe; `header` also gets `contains` and, through a set, `index_of`
macro_rules! probe_keyset {
    ($tier:ident header { $($hit:literal => $value:literal),+ $(,)? } misses [$($miss:literal),+ $(,)?]) => {
        probe_keyset!(@get header { $($hit => $value),+ });

        crate::perfect_set!(static HEADER_SET = [$($hit),+];);

        #[unsafe(no_mangle)]
        #[inline(never)]
        pub fn perfect_hash_contains_header(key: &str) -> bool {
            HEADER.contains_key(key)
        }

        #[unsafe(no_mangle)]
        #[inline(never)]
        pub fn perfect_hash_index_of_header(key: &str) -> Option<usize> {
            HEADER_SET.index_of(key)
        }
    };
    ($tier:ident $name:ident { $($hit:literal => $value:literal),+ $(,)? } misses [$($miss:literal),+ $(,)?]) => {
        probe_keyset!(@get $name { $($hit => $value),+ });
    };
    (@get $name:ident { $($hit:literal => $value:literal),+ }) => {
        super::__paste! {
            crate::perfect_map!(static [<$name:upper>]: i32 = { $($hit => $value),+ };);

            #[unsafe(no_mangle)]
            #[inline(never)]
            pub fn [<perfect_hash_get_ $name>](key: &str) -> Option<i32> {
                [<$name:upper>].get(key).copied()
            }
        }
    };
}

// after the macro it expands to, `macro_rules!` being scoped textually
keysets!(probe_keyset);
