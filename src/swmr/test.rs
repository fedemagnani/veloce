use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::swmr::{SwmrError, reader::Reader, register::Register, writer::Writer};

#[test]
fn single_thread() -> Result<(), SwmrError> {
    let num_readers = 2;
    let num_slots = 4;

    let init_slot = |_| 0;
    let init_hp = |_| AtomicUsize::default();

    let register = Register::<u64, AtomicUsize>::new(num_slots, init_slot, num_readers, init_hp);

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
    let num_slots = 3;

    let register = Register::<u64, AtomicUsize>::new(
        num_slots,
        |_| 0,
        num_readers,
        |_| AtomicUsize::default(),
    );

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
    let num_slots = 3;

    let register = Arc::new(Register::<u64, AtomicUsize>::new(
        num_slots,
        |_| 0,
        num_readers,
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
    let current = register.current.load(Ordering::SeqCst);
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
    let current = register.current.load(Ordering::SeqCst);
    let pinned = hp.load(Ordering::SeqCst);
    assert_eq!(pinned, current);

    Ok(())
}
