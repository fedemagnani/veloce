# cargo-codegen

See the assembly your users get from your crate's hot paths.

- you write **probes**: small wrappers around your API, marked [`#[codegen::probe]`](../codegen)
- `cargo codegen` compiles them **in a generated downstream crate**, so inlining matches your users'
- then it lists, counts and diffs their assembly
- mark functions [`#[codegen::watch]`](../codegen) to see how many of each probe's instructions come from them

## Toy example

```toml
# Cargo.toml: the attributes are no-ops unless `cargo codegen` builds
[dependencies]
codegen = { path = "../codegen" }
```

```rust
// src/lib.rs
#[codegen::watch]
pub fn get(values: &[u64], i: usize) -> u64 {
    values[i]
}

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
watched        instr  branch  jump  select  load  store  simd  barrier  call  panic
my_get > get       8       1     0       0     1      0     0        0     1      1
```

## Commands

| Command | Does |
| --- | --- |
| `asm` | writes each probe's listing to `target/codegen/` |
| `stats` | counts branches, loads, stores, SIMD, barriers, calls, panics per probe and per watched function |
| `stats --mca` | adds [llvm-mca](https://llvm.org/docs/CommandGuide/llvm-mca.html)'s estimates on the host CPU, or `--mcpu <cpu>` |
| `save` | saves a baseline |
| `diff` | compares to the baseline, exit 1 on drift |

Add `--target <triple>` for other targets. Run `cargo codegen --help` for every option.

## Reading `--mca`

llvm-mca runs each probe body as a straight-line loop: it ignores which branches are taken and models no mispredicts.

- `rthru`: cycles per call if only execution units limited it; the column that ranks probes best.
- `cycles`: simulated cycles per call, inflated when a return value feeds the next call's arguments.
- `deps%`, `press%`, `unit`: what stalled dispatch, and the busiest execution unit.
- `critical`: instructions on the longest dependency chain.

On veloce's perfect_hash lookups, `rthru` ranks hit paths as criterion measures them (Spearman 0.90), misses loosely, mixed streams not at all.
Needs `brew install llvm`, or `LLVM_MCA` set to the binary.
