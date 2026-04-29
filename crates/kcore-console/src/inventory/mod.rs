//! Host inventory: network, disks, metadata, API probe, diagnostics.

pub mod api;
pub mod diagnostics;
pub mod disk;
pub mod format;
pub mod network;
pub mod route;
pub mod system;

use self::system::Meta;

/// Full snapshot for one UI refresh.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub meta: Meta,
    pub nics: Vec<network::Nic>,
    pub disks: Vec<disk::Disk>,
    pub diag: Vec<diagnostics::ServiceLine>,
}

/// Collects inventory (may be partially empty on errors).
pub fn load_snapshot() -> Snapshot {
    let api = api::probe();
    let nics = network::list_nics();
    let disks = disk::list_disks();
    let diag = diagnostics::kcore_diagnostics();
    let meta = system::load_meta(&api);
    Snapshot {
        meta,
        nics,
        disks,
        diag,
    }
}
