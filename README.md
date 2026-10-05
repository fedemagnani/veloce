# veloce

Performant data structures in Rust

## Modules

| Module                    | Description                                                                                         |
| ------------------------- | --------------------------------------------------------------------------------------------------- |
| [`spsc`](src/spsc/mod.rs) | lock-free single-producer single-consumer channel with async support using ring-buffer and atomics. |
| [`swmr`](src/swmr/mod.rs) | triple-buffer inspired single-writer multi-reader register, publishing the latest value without copies. |


## Benchmarks

Live benchmarks can be found [here](https://fedemagnani.github.io/veloce/dev/bench/)
