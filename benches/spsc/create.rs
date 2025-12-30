//! # Create — Channel Allocation Cost
//!
//! **Real-world scenario**: Connection pools, per-request channels, or any
//! pattern where channels are created frequently.
//!
//! ```text
//! for request in requests {
//!     let (tx, rx) = channel();    // ← How fast is this?
//!     handle(request, tx, rx);
//! }
//! ```
//!
//! **What matters**: If you're creating channels in a hot loop, allocation
//! cost dominates. veloce is **~15× faster** than alternatives.
//!
//! ## Why veloce is faster
//!
//! - **Compile-time size**: Buffer size is a const generic, no runtime decisions
//! - **Stack-friendly**: Small channels can avoid heap allocation entirely
//! - **Minimal bookkeeping**: SPSC needs less internal state than MPMC

use crossbeam_channel::bounded as crossbeam_bounded;
use std::sync::mpsc::sync_channel as std_sync_channel;
use test::Bencher;
use veloce::spsc::channel;
const BUFFER_SIZE: usize = 1024;

#[bench]
fn veloce(b: &mut Bencher) {
    b.iter(channel::<i32, BUFFER_SIZE>);
}

#[bench]
fn crossbeam(b: &mut Bencher) {
    b.iter(|| crossbeam_bounded::<i32>(BUFFER_SIZE));
}

#[bench]
fn std_sync(b: &mut Bencher) {
    b.iter(|| std_sync_channel::<i32>(BUFFER_SIZE));
}
