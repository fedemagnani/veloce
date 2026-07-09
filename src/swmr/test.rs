use std::sync::atomic::AtomicUsize;

use crate::swmr::{SwmrError, register::Register};

#[test]
fn single_thread() -> Result<(), SwmrError> {
    let num_readers = 2;
    let num_slots = 4;

    let init_slot = |_| 0;
    let init_hp = |_| AtomicUsize::default();

    let register = Register::<u64, AtomicUsize>::new(num_slots, init_slot, num_readers, init_hp);

    let (mut writer, readers) = register.split();

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
