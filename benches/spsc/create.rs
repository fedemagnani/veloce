//! Channel creation overhead: measure allocation and initialization cost.
//!
//! Tests how fast new channels can be created. Relevant when channels are
//! created frequently in hot paths.

use crossbeam_channel::bounded as crossbeam_bounded;
use flume::bounded as flume_bounded;
use kanal::bounded as kanal_bounded;
use std::sync::mpsc::sync_channel as std_sync_channel;
use test::Bencher;
use veloce::spsc::lamport::channel as lamport_channel;
use veloce::spsc::vyukov::channel as vyukov_channel;
const BUFFER_SIZE: usize = 1024;

#[bench]
fn veloce_lamport(b: &mut Bencher) {
    b.iter(lamport_channel::<i32, BUFFER_SIZE>);
}

#[bench]
fn veloce_vyukov(b: &mut Bencher) {
    b.iter(vyukov_channel::<i32, BUFFER_SIZE>);
}

#[bench]
fn crossbeam(b: &mut Bencher) {
    b.iter(|| crossbeam_bounded::<i32>(BUFFER_SIZE));
}

#[bench]
fn std_sync(b: &mut Bencher) {
    b.iter(|| std_sync_channel::<i32>(BUFFER_SIZE));
}

#[bench]
fn flume(b: &mut Bencher) {
    b.iter(|| flume_bounded::<i32>(BUFFER_SIZE));
}

#[bench]
fn kanal(b: &mut Bencher) {
    b.iter(|| kanal_bounded::<i32>(BUFFER_SIZE));
}
