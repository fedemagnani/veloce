//! SPSC Channel Benchmarks: veloce vs crossbeam vs std
//!
//! Run with: cargo +nightly bench --bench spsc_comparison
//!
//! These benchmarks compare single-producer single-consumer channel performance
//! across three implementations with equivalent bounded capacity where possible.

#![feature(test)]

extern crate test;

mod burst;
mod create;
mod latency;
mod oneshot;
mod seq_inout;
mod small_buffer;
mod throughput;

pub use crossbeam_channel::bounded as crossbeam_bounded;
pub use crossbeam_utils::thread::scope;
pub use std::sync::mpsc::sync_channel as std_sync_channel;
pub use test::Bencher;
pub use veloce::spsc::channel;

pub const BUFFER_SIZE: usize = 1024;
pub const TOTAL_MESSAGES: usize = 100_000;
