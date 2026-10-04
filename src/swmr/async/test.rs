use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    task::{Context, Poll},
    thread,
    time::Duration,
};

use futures::{
    FutureExt,
    task::{ArcWake, waker},
};

use super::Async;
use crate::swmr::{SwmrError, reader::Reader, register::Register, writer::Writer};

/// Waker counting how many times it has been woken
struct WakeCounter(AtomicUsize);

impl ArcWake for WakeCounter {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn wake_counter() -> Arc<WakeCounter> {
    let count = AtomicUsize::new(0);
    Arc::new(WakeCounter(count))
}

/// `changed` resolves only on a value not yet returned by `latest`, the initial one included
#[test]
fn changed_resolves_on_unseen_value() -> Result<(), SwmrError> {
    let num_readers = 1;

    let register = Register::<u64, Async>::new(num_readers, || 0);

    let (mut writer, mut readers) = register.split();
    let reader = &mut readers[0];

    let initial = reader.changed().now_or_never();
    assert_eq!(initial, Some(Ok(())));
    let latest = *reader.latest();
    assert_eq!(latest, 0);

    let seen = reader.changed().now_or_never();
    assert_eq!(seen, None);

    writer.stage(|stored| *stored = 1);
    writer.commit()?;

    let committed = reader.changed().now_or_never();
    assert_eq!(committed, Some(Ok(())));

    Ok(())
}

/// A commit wakes the waiting reader only, and stops once its future is dropped
#[test]
fn commit_wakes_waiting_reader() -> Result<(), SwmrError> {
    let num_readers = 1;

    let register = Arc::new(Register::<u64, Async>::new(num_readers, || 0));
    let mut writer = Writer::from(register.clone());
    let mut reader = Reader::new(register.clone(), 0);
    let strategy = &register.busy_slots[0];

    let counter = wake_counter();
    let waker = waker(counter.clone());
    let mut cx = Context::from_waker(&waker);

    reader.latest();
    let mut changed = reader.changed();
    let poll = Pin::new(&mut changed).poll(&mut cx);
    assert_eq!(poll, Poll::Pending);
    let waiting = strategy.waiting.load(Ordering::SeqCst);
    assert!(waiting);

    writer.stage(|stored| *stored = 1);
    writer.commit()?;
    let wakes = counter.0.load(Ordering::SeqCst);
    assert_eq!(wakes, 1);

    let poll = Pin::new(&mut changed).poll(&mut cx);
    assert_eq!(poll, Poll::Ready(Ok(())));
    drop(changed);
    let waiting = strategy.waiting.load(Ordering::SeqCst);
    assert!(!waiting);

    // no reader is waiting, so the writer doesn't wake anyone
    writer.stage(|stored| *stored = 2);
    writer.commit()?;
    let wakes = counter.0.load(Ordering::SeqCst);
    assert_eq!(wakes, 1);

    Ok(())
}

/// Dropping the writer wakes the waiting reader, which then observes the disconnection
#[test]
fn writer_drop_wakes_waiting_reader() {
    let num_readers = 1;

    let register = Arc::new(Register::<u64, Async>::new(num_readers, || 0));
    let writer = Writer::from(register.clone());
    let mut reader = Reader::new(register.clone(), 0);

    let counter = wake_counter();
    let waker = waker(counter.clone());
    let mut cx = Context::from_waker(&waker);

    reader.latest();
    let mut changed = reader.changed();
    let poll = Pin::new(&mut changed).poll(&mut cx);
    assert_eq!(poll, Poll::Pending);

    drop(writer);
    let wakes = counter.0.load(Ordering::SeqCst);
    assert_eq!(wakes, 1);

    let poll = Pin::new(&mut changed).poll(&mut cx);
    assert_eq!(poll, Poll::Ready(Err(SwmrError::Disconnected)));
}

/// A value committed right before dropping the writer is reported before the disconnection
#[test]
fn changed_reports_final_value_before_disconnection() -> Result<(), SwmrError> {
    let num_readers = 1;

    let register = Register::<u64, Async>::new(num_readers, || 0);

    let (mut writer, mut readers) = register.split();
    let reader = &mut readers[0];
    reader.latest();

    writer.stage(|stored| *stored = 1);
    writer.commit()?;
    drop(writer);

    let last = reader.changed().now_or_never();
    assert_eq!(last, Some(Ok(())));
    let latest = *reader.latest();
    assert_eq!(latest, 1);

    let closed = reader.changed().now_or_never();
    assert_eq!(closed, Some(Err(SwmrError::Disconnected)));

    Ok(())
}

/// Readers awaiting on other threads never miss a wake-up, observing the final value
#[test]
fn no_lost_wakeups_across_threads() -> Result<(), SwmrError> {
    let num_readers = 4;
    let num_commits = 10_000;

    let register = Register::<u64, Async>::new(num_readers, || 0);
    let (mut writer, readers) = register.split();

    let (done_tx, done_rx) = mpsc::channel();
    for mut reader in readers {
        let done_tx = done_tx.clone();
        thread::spawn(move || {
            let observe = async {
                let mut last = 0;
                while reader.changed().await.is_ok() {
                    let latest = *reader.latest();
                    assert!(latest >= last, "values must be observed in commit order");
                    last = latest;
                }
                last
            };
            let last = futures::executor::block_on(observe);
            let _ = done_tx.send(last);
        });
    }
    drop(done_tx);

    for v in 1..=num_commits {
        writer.stage(|stored| *stored = v);
        writer.commit()?;
    }
    drop(writer);

    // a lost wake-up leaves the reader parked forever, so we bound the wait
    let timeout = Duration::from_secs(10);
    for _ in 0..num_readers {
        let last = done_rx
            .recv_timeout(timeout)
            .expect("reader missed a wake-up");
        assert_eq!(last, num_commits);
    }

    Ok(())
}
