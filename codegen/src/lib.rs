//! Assembly-aware analysis of veloce's hot paths
//!
//! Every `#[unsafe(no_mangle)]` function under [`probes`] is a probe: a never-inlined wrapper of veloce's public API,
//! found by symbol in the emitted assembly; `cargo run -p codegen -- asm` writes one listing per probe.

pub mod probes;
