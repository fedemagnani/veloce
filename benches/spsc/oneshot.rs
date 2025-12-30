//! Oneshot pattern: create channel, send one message, receive, discard.
//!
//! Measures total cost including channel creation and teardown. Relevant when
//! channels are created frequently and used only once.
//!
//! Real-world scenarios:
//! - Async task result delivery (futures/promises)
//! - Per-request response channels
//! - Thread spawn with result callback

use crossbeam_channel::bounded as crossbeam_bounded;
use flume::bounded as flume_bounded;
use kanal::bounded as kanal_bounded;
use std::sync::mpsc::sync_channel as std_sync_channel;
use test::Bencher;
use veloce::spsc::channel;

const BUFFER_SIZE: usize = 1024;
#[bench]
fn veloce(b: &mut Bencher) {
    b.iter(|| {
        let (tx, rx) = channel::<i32, BUFFER_SIZE>();
        tx.try_send(42).unwrap();
        rx.try_recv().unwrap()
    });
}

#[bench]
fn crossbeam(b: &mut Bencher) {
    b.iter(|| {
        let (tx, rx) = crossbeam_bounded::<i32>(BUFFER_SIZE);
        tx.send(42).unwrap();
        rx.recv().unwrap()
    });
}

#[bench]
fn std_sync(b: &mut Bencher) {
    b.iter(|| {
        let (tx, rx) = std_sync_channel::<i32>(BUFFER_SIZE);
        tx.send(42).unwrap();
        rx.recv().unwrap()
    });
}

#[bench]
fn flume(b: &mut Bencher) {
    b.iter(|| {
        let (tx, rx) = flume_bounded::<i32>(BUFFER_SIZE);
        tx.send(42).unwrap();
        rx.recv().unwrap()
    });
}

#[bench]
fn kanal(b: &mut Bencher) {
    b.iter(|| {
        let (tx, rx) = kanal_bounded::<i32>(BUFFER_SIZE);
        tx.send(42).unwrap();
        rx.recv().unwrap()
    });
}
