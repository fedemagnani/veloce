//! Probes of the swmr hot paths for `cargo codegen`, expanded in a downstream crate it generates

#[codegen::probe]
mod swmr {
    use crate::swmr::{Reader, SwmrError, Writer};

    /// Word-sized payload
    type P8 = u64;
    /// Page-sized payload, where copies dominate
    type P4K = [u64; 512];

    fn swmr_publish_p8(writer: &mut Writer<P8>, value: P8) -> Result<(), SwmrError> {
        writer.publish(value)
    }

    fn swmr_publish_p4k(writer: &mut Writer<P4K>, value: P4K) -> Result<(), SwmrError> {
        writer.publish(value)
    }

    fn swmr_latest_p8(reader: &mut Reader<P8>) -> P8 {
        *reader.latest()
    }
}

// gated here, as a `#[cfg]` inside a probe `mod` would be evaluated in the downstream crate
#[cfg(feature = "async")]
#[codegen::probe]
mod swmr_async {
    use crate::swmr::{AsyncReader, AsyncWriter, SwmrError};

    fn swmr_publish_async_p8(writer: &mut AsyncWriter<u64>, value: u64) -> Result<(), SwmrError> {
        writer.publish(value)
    }

    fn swmr_latest_async_p8(reader: &mut AsyncReader<u64>) -> u64 {
        *reader.latest()
    }
}
