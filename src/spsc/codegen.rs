//! Probes of the spsc hot paths for the codegen tool, at the payload and capacity of the throughput benches

use super::{TryRecvError, TrySendErr, lamport, vyukov};

/// Payload of the throughput benches
pub type Item = i32;
/// Capacity of the throughput benches
pub const CAPACITY: usize = 1024;

#[unsafe(no_mangle)]
#[inline(never)]
pub fn spsc_lamport_try_send(
    sender: &lamport::Sender<Item, CAPACITY>,
    value: Item,
) -> Result<(), TrySendErr<Item>> {
    sender.try_send(value)
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn spsc_lamport_try_recv(
    receiver: &lamport::Receiver<Item, CAPACITY>,
) -> Result<Item, TryRecvError> {
    receiver.try_recv()
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn spsc_lamport_send_spin(
    sender: &lamport::Sender<Item, CAPACITY>,
    value: Item,
) -> Result<(), TrySendErr<Item>> {
    sender.send_spin(value)
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn spsc_lamport_recv_spin(
    receiver: &lamport::Receiver<Item, CAPACITY>,
) -> Result<Item, TryRecvError> {
    receiver.recv_spin()
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn spsc_lamport_drain_sum(
    receiver: &mut lamport::Receiver<Item, CAPACITY>,
    max: usize,
) -> Item {
    receiver.drain(max).fold(0, Item::wrapping_add)
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn spsc_vyukov_try_send(
    sender: &vyukov::Sender<Item, CAPACITY>,
    value: Item,
) -> Result<(), TrySendErr<Item>> {
    sender.try_send(value)
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn spsc_vyukov_try_recv(
    receiver: &vyukov::Receiver<Item, CAPACITY>,
) -> Result<Item, TryRecvError> {
    receiver.try_recv()
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn spsc_vyukov_send_spin(
    sender: &vyukov::Sender<Item, CAPACITY>,
    value: Item,
) -> Result<(), TrySendErr<Item>> {
    sender.send_spin(value)
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn spsc_vyukov_recv_spin(
    receiver: &vyukov::Receiver<Item, CAPACITY>,
) -> Result<Item, TryRecvError> {
    receiver.recv_spin()
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn spsc_vyukov_drain_sum(receiver: &mut vyukov::Receiver<Item, CAPACITY>, max: usize) -> Item {
    receiver.drain(max).fold(0, Item::wrapping_add)
}
