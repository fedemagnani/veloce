use crate::swmr::{Polling, SwmrError, register, register_with};

/// Every slot starts as `initial`: each stage lands on a slot never written, while written ones stay busy
#[test]
fn every_slot_starts_from_initial() -> Result<(), SwmrError> {
    let initial = 7;

    let (mut writer, [mut r0, mut r1]) = register::<u64, 2>(initial);

    // r0 pins the slot holding the initial value
    let latest = *r0.latest();
    assert_eq!(latest, initial);

    let mut staged = 0;
    writer.stage(|slot| {
        staged = *slot;
        *slot = 1;
    });
    assert_eq!(staged, initial);
    writer.commit()?;

    // r1 pins the slot of the first commit
    let latest = *r1.latest();
    assert_eq!(latest, 1);

    for v in 2..=3 {
        let mut staged = 0;
        writer.stage(|slot| {
            staged = *slot;
            *slot = v;
        });
        assert_eq!(staged, initial);
        writer.commit()?;
    }

    Ok(())
}

/// `register_with` accepts non-Clone values, calling `init` once per slot
#[test]
fn register_with_inits_each_slot() {
    struct NotClone(usize);

    let mut num_inits = 0;
    let init = || {
        num_inits += 1;
        NotClone(num_inits)
    };

    let (_writer, [mut reader]) = register_with::<_, Polling, 1>(init);

    let num_slots = 3;
    assert_eq!(num_inits, num_slots);
    let latest = reader.latest().0;
    assert_eq!(latest, 1);
}

/// `publish` commits right away, so every reader sees the value
#[test]
fn publish_reaches_readers() -> Result<(), SwmrError> {
    let (mut writer, mut readers) = register::<u64, 2>(0);

    writer.publish(1)?;

    for reader in &mut readers {
        let latest = *reader.latest();
        assert_eq!(latest, 1);
        let version = reader.version();
        assert_eq!(version, Some(1));
    }

    Ok(())
}

/// `publish` overrides a value staged and not yet committed, within a single commit
#[test]
fn publish_overrides_staged() -> Result<(), SwmrError> {
    let (mut writer, [mut reader]) = register::<u64, 1>(0);

    writer.stage(|slot| *slot = 1);
    writer.publish(2)?;

    let latest = *reader.latest();
    assert_eq!(latest, 2);
    let version = reader.version();
    assert_eq!(version, Some(1));

    Ok(())
}

/// `update` builds each value from the latest committed one
#[test]
fn update_sees_latest() -> Result<(), SwmrError> {
    let (mut writer, [mut reader]) = register::<u64, 1>(0);

    let num_updates = 10;
    for _ in 0..num_updates {
        writer.update(|slot, latest| *slot = latest + 1)?;
    }

    let latest = *reader.latest();
    assert_eq!(latest, num_updates);

    Ok(())
}

/// Once every reader is dropped, the shortcuts error without running the setter
#[test]
fn shortcuts_error_once_readers_dropped() {
    let (mut writer, [reader]) = register::<u64, 1>(0);
    drop(reader);

    let published = writer.publish(1);
    assert_eq!(published, Err(SwmrError::Disconnected));

    let mut setter_ran = false;
    let updated = writer.update(|_, _| setter_ran = true);
    assert_eq!(updated, Err(SwmrError::Disconnected));
    assert!(!setter_ran);
}
