//! Single-Producer Single-Consumer (SPSC) Channels
//!
//! Lock-free bounded channels built on ring buffers for fast, allocation-free
//! message passing between exactly one producer and one consumer thread.
//!
//! ## Implementations
//!
//! This module provides two alternative SPSC algorithms:
//!
//! - [`lamport`] — Classic approach with shared atomic head/tail indices
//! - [`vyukov`] — Per-slot sequence stamps for reduced cache contention

pub mod lamport;
pub mod vyukov;
