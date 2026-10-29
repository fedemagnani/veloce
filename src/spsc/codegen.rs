//! Probes of the spsc hot paths for `cargo codegen`, at the payload and capacity of the throughput benches

#[codegen::probe]
mod spsc {
    use crate::spsc::{TryRecvError, TrySendErr, lamport, vyukov};

    /// Payload of the throughput benches
    type Item = i32;
    /// Capacity of the throughput benches
    const CAPACITY: usize = 1024;

    fn spsc_lamport_try_send(
        sender: &lamport::Sender<Item, CAPACITY>,
        value: Item,
    ) -> Result<(), TrySendErr<Item>> {
        sender.try_send(value)
    }

    fn spsc_lamport_try_recv(
        receiver: &lamport::Receiver<Item, CAPACITY>,
    ) -> Result<Item, TryRecvError> {
        receiver.try_recv()
    }

    fn spsc_lamport_send_spin(
        sender: &lamport::Sender<Item, CAPACITY>,
        value: Item,
    ) -> Result<(), TrySendErr<Item>> {
        sender.send_spin(value)
    }

    fn spsc_lamport_recv_spin(
        receiver: &lamport::Receiver<Item, CAPACITY>,
    ) -> Result<Item, TryRecvError> {
        receiver.recv_spin()
    }

    fn spsc_lamport_drain_sum(
        receiver: &mut lamport::Receiver<Item, CAPACITY>,
        max: usize,
    ) -> Item {
        receiver.drain(max).fold(0, Item::wrapping_add)
    }

    fn spsc_vyukov_try_send(
        sender: &vyukov::Sender<Item, CAPACITY>,
        value: Item,
    ) -> Result<(), TrySendErr<Item>> {
        sender.try_send(value)
    }

    fn spsc_vyukov_try_recv(
        receiver: &vyukov::Receiver<Item, CAPACITY>,
    ) -> Result<Item, TryRecvError> {
        receiver.try_recv()
    }

    fn spsc_vyukov_send_spin(
        sender: &vyukov::Sender<Item, CAPACITY>,
        value: Item,
    ) -> Result<(), TrySendErr<Item>> {
        sender.send_spin(value)
    }

    fn spsc_vyukov_recv_spin(
        receiver: &vyukov::Receiver<Item, CAPACITY>,
    ) -> Result<Item, TryRecvError> {
        receiver.recv_spin()
    }

    fn spsc_vyukov_drain_sum(receiver: &mut vyukov::Receiver<Item, CAPACITY>, max: usize) -> Item {
        receiver.drain(max).fold(0, Item::wrapping_add)
    }
}
