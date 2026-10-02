use std::sync::atomic::AtomicUsize;

use crate::swmr::{SwmrError, register::Register};

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
