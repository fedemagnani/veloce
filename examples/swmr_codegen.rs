//! Non-generic instances of the swmr hot paths, so `scripts/swmr-codegen.sh` can disassemble them by symbol

use veloce::swmr::{AsyncReader, AsyncWriter, Reader, SwmrError, Writer};

/// Word-sized payload
pub type P8 = u64;
/// Page-sized payload, where copies dominate
pub type P4K = [u64; 512];

#[unsafe(no_mangle)]
#[inline(never)]
pub fn swmr_publish_p8(writer: &mut Writer<P8>, value: P8) -> Result<(), SwmrError> {
    writer.publish(value)
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn swmr_publish_p4k(writer: &mut Writer<P4K>, value: P4K) -> Result<(), SwmrError> {
    writer.publish(value)
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn swmr_latest_p8(reader: &mut Reader<P8>) -> P8 {
    *reader.latest()
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn swmr_publish_async_p8(writer: &mut AsyncWriter<P8>, value: P8) -> Result<(), SwmrError> {
    writer.publish(value)
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn swmr_latest_async_p8(reader: &mut AsyncReader<P8>) -> P8 {
    *reader.latest()
}

fn main() {}
