//! Probes of the perfect_hash lookups for `cargo codegen`, one `get` per bench key set so every compare path has one

// the key sets of the benches, so the probes cover exactly what the benches measure
include!("../../benches/perfect_hash/keysets.rs");

/// Collects a map and a `get` probe per key set, plus `contains` and `index_of` on `header`, into one probe block
macro_rules! probe_keysets {
    (@probes [$($probes:tt)*]) => {
        $crate::perfect_hash::__paste! {
            ::codegen::probes!(perfect_hash_probes { $($probes)* });
        }
    };
    (
        @probes [$($probes:tt)*]
        $tier:ident header { $($hit:literal => $value:literal),+ $(,)? } misses $misses:tt
        $($rest:tt)*
    ) => {
        probe_keysets!(@probes [
            $($probes)*

            // `index_of` is only on sets, so it gets a set of the same keys
            $crate::perfect_set!(static HEADER_SET = [$($hit),+];);

            fn perfect_hash_contains_header(key: &str) -> bool {
                HEADER.contains_key(key)
            }

            fn perfect_hash_index_of_header(key: &str) -> Option<usize> {
                HEADER_SET.index_of(key)
            }
        ] $tier @get header { $($hit => $value),+ } misses $misses $($rest)*);
    };
    (@probes [$($probes:tt)*] $tier:ident $name:ident $hits:tt misses $misses:tt $($rest:tt)*) => {
        probe_keysets!(@probes [$($probes)*] $tier @get $name $hits misses $misses $($rest)*);
    };
    (
        @probes [$($probes:tt)*]
        $tier:ident @get $name:ident { $($hit:literal => $value:literal),+ $(,)? } misses $misses:tt
        $($rest:tt)*
    ) => {
        probe_keysets!(@probes [
            $($probes)*

            $crate::perfect_map!(static [<$name:upper>]: i32 = { $($hit => $value),+ };);

            fn [<perfect_hash_get_ $name>](key: &str) -> Option<i32> {
                [<$name:upper>].get(key).copied()
            }
        ] $($rest)*);
    };
    ($($sets:tt)*) => {
        probe_keysets!(@probes [] $($sets)*);
    };
}

// after the macros it expands to, `macro_rules!` being scoped textually
keysets!(probe_keysets);
