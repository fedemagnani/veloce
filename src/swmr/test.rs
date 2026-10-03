use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::swmr::{
    SwmrError,
    reader::Reader,
    register::{Register, SlotHeader},
    writer::Writer,
};

#[test]
fn single_thread() -> Result<(), SwmrError> {
    let num_readers = 2;

    let init_slot = |_| 0;
    let init_hp = |_| AtomicUsize::default();

    let register = Register::<u64, AtomicUsize>::new(num_readers, init_slot, init_hp);

    let (mut writer, mut readers) = register.split();

    assert_eq!(readers.len(), 2);

    let slot_updater = |old_val: &mut u64, new_val: u64| *old_val = new_val;

    let val = 41;
    writer.stage(|stored| slot_updater(stored, val));
    let new_val = 42;
    writer.stage(|stored| slot_updater(stored, new_val));
    writer.commit()?;

    let l0 = readers[0].latest();
    assert_eq!(l0, &new_val);

    let l1 = readers[1].latest();
    assert_eq!(l1, &new_val);

    Ok(())
}

/// The reference to the latest value at time remains the same even if in the meanwhile
/// the producer has published a new value
#[test]
fn held_reference_survives_publishes() -> Result<(), SwmrError> {
    let num_readers = 1;

    let register =
        Register::<u64, AtomicUsize>::new(num_readers, |_| 0, |_| AtomicUsize::default());

    let (mut writer, mut readers) = register.split();
    let reader = &mut readers[0];

    writer.stage(|stored| *stored = 1);
    writer.commit()?;

    let held = reader.latest();

    // more publishes than slots, forcing recycling
    for v in 2..10 {
        writer.stage(|stored| *stored = v);
        writer.commit()?;
    }

    assert_eq!(held, &1);
    assert_eq!(reader.latest(), &9);

    Ok(())
}

/// Without new publishes, the reader must not touch its hazard pointer:
/// we tamper with it and check that `latest` doesn't restore it
#[test]
fn fast_path_skips_hazard_store() -> Result<(), SwmrError> {
    let num_readers = 1;
    let num_slots = num_readers + 2;

    let register = Arc::new(Register::<u64, AtomicUsize>::new(
        num_readers,
        |_| 0,
        |_| AtomicUsize::default(),
    ));
    let mut writer = Writer::from(register.clone());
    let mut reader = Reader::new(register.clone(), 0);
    let hp = &register.busy_slots[0];

    writer.stage(|stored| *stored = 1);
    writer.commit()?;

    // slow path: pins the current slot
    let latest = *reader.latest();
    assert_eq!(latest, 1);
    let current = register.current.load(Ordering::SeqCst).slot();
    let pinned = hp.load(Ordering::SeqCst);
    assert_eq!(pinned, current);

    // fast path: the hazard pointer is left untouched
    let tampered = (current + 1) % num_slots;
    hp.store(tampered, Ordering::SeqCst);
    let latest = *reader.latest();
    assert_eq!(latest, 1);
    let pinned = hp.load(Ordering::SeqCst);
    assert_eq!(pinned, tampered);

    // a new publish forces the slow path, which pins the new current slot
    writer.stage(|stored| *stored = 2);
    writer.commit()?;
    let latest = *reader.latest();
    assert_eq!(latest, 2);
    let current = register.current.load(Ordering::SeqCst).slot();
    let pinned = hp.load(Ordering::SeqCst);
    assert_eq!(pinned, current);

    Ok(())
}

/// The version counts the commits, and tells the reader whether its held value is outdated
#[test]
fn version_tracks_commits() -> Result<(), SwmrError> {
    let num_readers = 1;

    let register =
        Register::<u64, AtomicUsize>::new(num_readers, |_| 0, |_| AtomicUsize::default());

    let (mut writer, mut readers) = register.split();
    let reader = &mut readers[0];

    // the initial value has not been seen yet
    let changed = reader.has_changed();
    assert!(changed);
    let version = reader.version();
    assert_eq!(version, None);

    let initial = *reader.latest();
    assert_eq!(initial, 0);
    let changed = reader.has_changed();
    assert!(!changed);
    let version = reader.version();
    assert_eq!(version, Some(0));

    let num_commits = 5;
    for v in 1..=num_commits {
        writer.stage(|stored| *stored = v);
        writer.commit()?;
    }

    // `has_changed` doesn't pin: the version still refers to the held value
    let changed = reader.has_changed();
    assert!(changed);
    let version = reader.version();
    assert_eq!(version, Some(0));

    let latest = *reader.latest();
    assert_eq!(latest, num_commits);
    let changed = reader.has_changed();
    assert!(!changed);
    let version = reader.version();
    let expected = Some(num_commits as usize);
    assert_eq!(version, expected);

    Ok(())
}

/// Staging with the latest value allows delta updates, even after slots are recycled
#[test]
fn stage_with_latest_applies_deltas() -> Result<(), SwmrError> {
    let num_readers = 2;

    let register = Register::<Vec<u64>, AtomicUsize>::new(
        num_readers,
        |_| Vec::new(),
        |_| AtomicUsize::default(),
    );

    let (mut writer, mut readers) = register.split();

    // more publishes than slots, so each staged slot starts from a stale value
    let num_commits = 10;
    for v in 1..=num_commits {
        writer.stage_with_latest(|slot, latest| {
            slot.clone_from(latest);
            slot.push(v);
        });
        writer.commit()?;
    }

    let expected: Vec<u64> = (1..=num_commits).collect();
    for reader in &mut readers {
        let latest = reader.latest();
        assert_eq!(latest, &expected);
    }

    Ok(())
}

/// Overflowing the version must wrap it to zero, leaving the slot index and the closed mark untouched
#[test]
fn slot_info_version_wraps() {
    let max_version = usize::MAX >> SlotHeader::VERSION_SHIFT;
    let last = SlotHeader::new(max_version, 3);
    let last_version = last.version();
    assert_eq!(last_version, max_version);
    let last_closed = last.is_closed();
    assert!(!last_closed);

    let next_slot = 5;
    let wrapped = last.next(next_slot);
    let wrapped_version = wrapped.version();
    assert_eq!(wrapped_version, 0);
    let wrapped_slot = wrapped.slot();
    assert_eq!(wrapped_slot, next_slot);
    let wrapped_closed = wrapped.is_closed();
    assert!(!wrapped_closed);
}

/// Dropping the writer closes the register: readers keep reading the last committed value,
/// while a staged value not yet committed is discarded
#[test]
fn writer_drop_closes_register() -> Result<(), SwmrError> {
    let num_readers = 2;

    let register =
        Register::<u64, AtomicUsize>::new(num_readers, |_| 0, |_| AtomicUsize::default());

    let (mut writer, mut readers) = register.split();

    writer.stage(|stored| *stored = 1);
    writer.commit()?;
    writer.stage(|stored| *stored = 2);

    for reader in &readers {
        let closed = reader.is_closed();
        assert!(!closed);
    }

    drop(writer);

    for reader in &mut readers {
        let closed = reader.is_closed();
        assert!(closed);
        let latest = *reader.latest();
        assert_eq!(latest, 1);
        let version = reader.version();
        assert_eq!(version, Some(1));
    }

    Ok(())
}
