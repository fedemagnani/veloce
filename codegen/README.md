# codegen

Defines probes for [`cargo-codegen`](../cargo-codegen): never-inlined wrappers of your crate's API.
A plain dependency: every attribute is a no-op unless `cargo codegen` builds.

```rust
#[codegen::probe]
fn my_get(values: &[u64], i: usize) -> u64 {
    crate::get(values, i)
}

#[codegen::probe]
mod my_probes {
    use crate::get;

    fn my_first(values: &[u64]) -> u64 {
        get(values, 0)
    }
}
```

- reach your crate through `crate::` paths
- probes must be concrete: no generics
- gate a whole `fn` or `mod` with `#[cfg]`, not items inside a `mod`
- generating probes from a macro? use `codegen::probes!(name { .. })`, with `$crate::` paths
- `#[codegen::watch]` on any `fn` counts its share of every probe that inlines it
