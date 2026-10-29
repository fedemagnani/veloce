# cargo-codegen

See the assembly your users get from your crate's hot paths.

- you write **probes**: small wrappers around your API, marked [`#[codegen::probe]`](../codegen)
- `cargo codegen` compiles them **in a generated downstream crate**, so inlining matches your users'
- then it lists, counts and diffs their assembly

## Toy example

```toml
# Cargo.toml
[dependencies]
codegen = { path = "../codegen", optional = true }

[features]
codegen = ["dep:codegen"]
```

```rust
// src/lib.rs
pub fn get(values: &[u64], i: usize) -> u64 {
    values[i]
}

#[cfg(feature = "codegen")]
#[codegen::probe]
fn my_get(values: &[u64], i: usize) -> u64 {
    crate::get(values, i)
}
```

```sh
$ cargo install --path cargo-codegen
$ cargo codegen stats
== aarch64-apple-darwin
probe   instr  branch  jump  select  load  store  simd  barrier  call  panic
my_get     11       1     0       0     1      1     0        0     1      1
my_get: panics core::panicking::panic_bounds_check
```

## Commands

| Command | Does |
| --- | --- |
| `asm` | writes each probe's listing to `target/codegen/` |
| `stats` | counts branches, loads, stores, SIMD, barriers, calls, panics per probe |
| `save` | saves a baseline |
| `diff` | compares to the baseline, exit 1 on drift |

Add `--target <triple>` for other targets. Run `cargo codegen --help` for every option.
