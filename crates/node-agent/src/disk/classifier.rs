//! Node-agent facade over the shared [`kcore_disko_types`] classifier.
//!
//! The authoritative logic lives in the `kcore-disko-types` crate so the
//! controller pre-flight and the node-agent apply path stay in lockstep. This
//! module simply re-exports the types the `apply_disk_layout_impl` handler
//! uses, so call sites keep working unchanged.

pub use kcore_disko_types::{
    classify_disk_layout, extract_target_devices, BlockDevice, LsblkSnapshot, Verdict,
};

#[allow(unused_imports)]
pub use kcore_disko_types::refusal;
