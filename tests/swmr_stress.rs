//! Torn-read stress: readers hold values across publishes, so a slot recycled while pinned shows up as a torn or changed value

use std::{hint::spin_loop, thread};

use veloce::swmr::{self, Reader, SwmrError, WaitStrategy};

/// Words per value: a memory page natively, a few words under Miri, which interprets every access
#[cfg(not(miri))]
const WORDS: usize = 512;
#[cfg(miri)]
const WORDS: usize = 4;

/// Values published by the writer before dropping
#[cfg(not(miri))]
const COMMITS: u64 = 20_000;
#[cfg(miri)]
const COMMITS: u64 = 30;

/// Times a reader re-checks a held value, giving the writer room to recycle slots meanwhile
#[cfg(not(miri))]
const HOLD_CHECKS: usize = 16;
#[cfg(miri)]
const HOLD_CHECKS: usize = 2;

/// Value whose words all carry the sequence number of its commit
type Page = [u64; WORDS];

fn page(seq: u64) -> Page {
    [seq; WORDS]
}

/// Returns the sequence number of `value`, asserting that all of its words agree on it
fn check_untorn(value: &Page) -> u64 {
    let seq = value[0];
    let untorn = value.iter().all(|&word| word == seq);
    assert!(untorn, "torn value: words disagree with {seq}");
    seq
}

/// Checks the value just pinned by `reader` while holding it, returning its sequence number
fn check_latest<W: WaitStrategy>(reader: &mut Reader<Page, W>, last: u64) -> u64 {
    let held = reader.latest();
    let seq = check_untorn(held);
    assert!(seq >= last, "went back from {last} to {seq}");
    for _ in 0..HOLD_CHECKS {
        let again = check_untorn(held);
        assert_eq!(again, seq, "held value changed");
        spin_loop();
    }
    // each publish is a single commit, so the version counts the sequence numbers
    let version = reader.version();
    let expected = Some(seq as usize);
    assert_eq!(version, expected);
    seq
}

/// Reads until the writer drops, returning the sequence number of the final value
fn poll_until_closed<W: WaitStrategy>(reader: &mut Reader<Page, W>) -> u64 {
    let mut last = 0;
    loop {
        // checked before reading, so that once closed the read below returns the final value
        let closed = reader.is_closed();
        last = check_latest(reader, last);
        if closed {
            return last;
        }
    }
}

/// Publishes every sequence number up to [`COMMITS`], then drops the writer
fn publish_all<W: WaitStrategy>(mut writer: swmr::Writer<Page, W>) -> Result<(), SwmrError> {
    for seq in 1..=COMMITS {
        let value = page(seq);
        writer.publish(value)?;
    }
    Ok(())
}

/// `N` polling readers check every value they read while the writer publishes
fn stress_polling<const N: usize>() -> Result<(), SwmrError> {
    let initial = page(0);
    let (writer, readers) = swmr::register::<Page, N>(initial);

    thread::scope(|s| {
        let handles = readers.map(|mut reader| s.spawn(move || poll_until_closed(&mut reader)));
        publish_all(writer)?;

        for handle in handles {
            let last = handle.join().expect("reader panicked");
            assert_eq!(last, COMMITS);
        }
        Ok(())
    })
}

#[test]
fn polling_single_reader() -> Result<(), SwmrError> {
    stress_polling::<1>()
}

#[test]
fn polling_three_readers() -> Result<(), SwmrError> {
    stress_polling::<3>()
}

/// Every slot is in use, covering the full 64-bit slot bitmap
#[test]
#[cfg_attr(miri, ignore = "too many threads to interpret")]
fn polling_max_readers() -> Result<(), SwmrError> {
    stress_polling::<62>()
}

#[cfg(feature = "async")]
mod r#async {
    use std::{sync::mpsc, thread};

    use futures::executor::block_on;
    use veloce::swmr::{self, AsyncReader, SwmrError};

    use super::{COMMITS, Page, check_latest, check_untorn, page, publish_all};

    /// Awaits each change until the writer drops, returning the sequence number of the final value
    async fn await_until_closed(reader: &mut AsyncReader<Page>) -> u64 {
        let mut last = 0;
        while reader.changed().await.is_ok() {
            last = check_latest(reader, last);
        }
        last
    }

    /// `N` async readers check every value they are woken for while the writer publishes
    fn stress_async<const N: usize>() -> Result<(), SwmrError> {
        let initial = page(0);
        let (writer, readers) = swmr::register_async::<Page, N>(initial);

        thread::scope(|s| {
            let handles = readers.map(|mut reader| {
                s.spawn(move || {
                    let awaiting = await_until_closed(&mut reader);
                    block_on(awaiting)
                })
            });
            publish_all(writer)?;

            for handle in handles {
                let last = handle.join().expect("reader panicked");
                assert_eq!(last, COMMITS);
            }
            Ok(())
        })
    }

    #[test]
    fn async_single_reader() -> Result<(), SwmrError> {
        stress_async::<1>()
    }

    #[test]
    fn async_three_readers() -> Result<(), SwmrError> {
        stress_async::<3>()
    }

    #[test]
    #[cfg_attr(miri, ignore = "too many threads to interpret")]
    fn async_max_readers() -> Result<(), SwmrError> {
        stress_async::<62>()
    }

    /// Rounds of the wake-up handshake: each races one publish against one reader starting to wait
    #[cfg(not(miri))]
    const HANDSHAKES: u64 = 10_000;
    #[cfg(miri)]
    const HANDSHAKES: u64 = 20;

    /// A lost wake-up deadlocks: the writer stays alive, blocked on the acknowledgement, so no drop wakes the reader
    #[test]
    fn async_wakeup_handshake() -> Result<(), SwmrError> {
        let initial = page(0);
        let (mut writer, [mut reader]) = swmr::register_async::<Page, 1>(initial);
        let (ack_tx, ack_rx) = mpsc::sync_channel(0);
        // seen before the writer starts, so each `changed` below can only resolve on the publish it races with
        reader.latest();

        // moved in, so a failing writer drops `ack_rx` and the blocked reader fails instead of hanging the scope
        thread::scope(move |s| {
            s.spawn(move || {
                block_on(async {
                    for _ in 1..=HANDSHAKES {
                        let changed = reader.changed().await;
                        changed.expect("writer alive");
                        let seq = check_untorn(reader.latest());
                        ack_tx.send(seq).expect("writer alive");
                    }
                })
            });

            for seq in 1..=HANDSHAKES {
                let value = page(seq);
                writer.publish(value)?;
                let acked = ack_rx.recv().expect("reader alive");
                assert_eq!(acked, seq);
            }
            Ok(())
        })
    }
}
