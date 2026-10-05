#![feature(test)]

extern crate test;

mod spsc {
    mod burst;
    mod create;
    mod latency;
    mod oneshot;
    mod seq_inout;
    mod slow_consumer;
    mod small_buffer;
    mod throughput;
}

mod swmr {
    #[cfg(feature = "async")]
    mod async_notify;
    mod common;
    mod create;
    mod propagation;
    mod read_contended;
    mod read_idle;
    mod write_contended;
}
