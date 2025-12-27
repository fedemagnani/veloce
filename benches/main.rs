#![feature(test)]

extern crate test;

mod spsc {
    mod burst;
    mod create;
    mod latency;
    mod oneshot;
    mod seq_inout;
    mod small_buffer;
    mod throughput;
}
