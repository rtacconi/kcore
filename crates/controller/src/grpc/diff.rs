//! Pure diff helpers for declarative upsert (server-side apply).
//!
//! Each `diff_*` function compares a stored row against an incoming spec and
//! returns two lists of field names:
//!   * `immutable` — fields whose value would change but which are declared
//!     immutable for v1; the caller MUST reject the apply with
//!     `InvalidArgument`.
//!   * `mutable` — fields whose value would change and are safe to update via
//!     existing `Update*` / `SetDesiredState` paths.
//!
//! Both vectors are empty when the stored state already matches the incoming
//! spec (no-op / `UNCHANGED`).
//!
//! Keep these helpers **pure**: no I/O, no DB, no clock. This lets every case
//! be unit-tested without a running controller.
//!
//! Mutable vs immutable policy (v1):
//!
//! | Kind          | Mutable                                  | Immutable                                                                                                                                   |
//! | ------------- | ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
//! | VM            | `cpu`, `memory_bytes`, `desired_state`   | `disks`, `nics`, `storage_backend`, `storage_size_bytes`, `target_node`, `target_dc`, `ssh_key_names`, `cloud_init_user_data`, `image_*`    |
//! | Container     | `env`, `ports`, `desired_state`          | `image`, `command`, `network`, `storage_backend`, `storage_size_bytes`, `mount_target`                                                      |
//! | Network       | *(none)*                                 | all fields                                                                                                                                  |
//! | SshKey        | *(none)*                                 | `public_key`                                                                                                                                |
//! | SecurityGroup | `description`, `rules`, `attachments`    | `name`                                                                                                                                      |

use std::collections::HashMap;

use crate::controller_proto;
use crate::db::{NetworkRow, SecurityGroupRow, SecurityGroupRuleRow, VmRow, WorkloadRow};

/// Result of diffing a stored row against an incoming spec.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SpecDiff {
    /// Fields whose change is rejected (caller returns `InvalidArgument`).
    pub immutable: Vec<String>,
    /// Fields whose change is allowed (caller applies via Update* / SetDesiredState).
    pub mutable: Vec<String>,
}

impl SpecDiff {
    pub fn is_empty(&self) -> bool {
        self.immutable.is_empty() && self.mutable.is_empty()
    }

    pub fn is_unchanged(&self) -> bool {
        self.is_empty()
    }
}

// ---------- VM ----------

/// Incoming VM apply payload (subset of `CreateVmRequest` relevant to diffing).
pub struct VmApply<'a> {
    pub spec: &'a controller_proto::VmSpec,
    pub image_url: &'a str,
    pub image_sha256: &'a str,
    pub image_path: &'a str,
    pub cloud_init_user_data: &'a str,
    pub ssh_key_names: &'a [String],
    pub storage_backend: &'a str,
    pub storage_size_bytes: i64,
    pub target_node: &'a str,
    pub target_dc: &'a str,
}

/// Compare a stored VM row against an incoming apply payload.
///
/// `stored_ssh_key_names` is passed separately because SSH-key attachments
/// live in a join table rather than on `VmRow`.
pub fn diff_vm(
    stored: &VmRow,
    stored_ssh_key_names: &[String],
    incoming: &VmApply<'_>,
) -> SpecDiff {
    let mut diff = SpecDiff::default();

    if stored.cpu != incoming.spec.cpu {
        diff.mutable.push("cpu".into());
    }
    if stored.memory_bytes != incoming.spec.memory_bytes {
        diff.mutable.push("memory_bytes".into());
    }

    let incoming_auto_start =
        match controller_proto::VmDesiredState::try_from(incoming.spec.desired_state)
            .unwrap_or(controller_proto::VmDesiredState::Unspecified)
        {
            controller_proto::VmDesiredState::Running => Some(true),
            controller_proto::VmDesiredState::Stopped => Some(false),
            controller_proto::VmDesiredState::Unspecified => None,
        };
    if let Some(want) = incoming_auto_start {
        if stored.auto_start != want {
            diff.mutable.push("desired_state".into());
        }
    }

    // Immutable fields -----------------------------------------------------

    // storage_backend: empty means "unspecified", skip check in that case.
    if !incoming.storage_backend.is_empty()
        && !stored.storage_backend.is_empty()
        && stored.storage_backend != incoming.storage_backend
    {
        diff.immutable.push("storage_backend".into());
    }
    if incoming.storage_size_bytes > 0 && stored.storage_size_bytes != incoming.storage_size_bytes {
        diff.immutable.push("storage_size_bytes".into());
    }

    // image_* — if either url or path is provided, it must match what was stored.
    if !incoming.image_url.is_empty() && stored.image_url != incoming.image_url {
        diff.immutable.push("image_url".into());
    }
    if !incoming.image_sha256.is_empty() && stored.image_sha256 != incoming.image_sha256 {
        diff.immutable.push("image_sha256".into());
    }
    if !incoming.image_path.is_empty()
        && !stored.image_path.is_empty()
        && stored.image_path != incoming.image_path
    {
        diff.immutable.push("image_path".into());
    }

    if !incoming.cloud_init_user_data.is_empty()
        && stored.cloud_init_user_data != incoming.cloud_init_user_data
    {
        diff.immutable.push("cloud_init_user_data".into());
    }

    // target_node: only reject if an explicit, different node was requested.
    if !incoming.target_node.is_empty() && stored.node_id != incoming.target_node {
        // Allow address-based match (target_node may be "host:port").
        // Controller resolves it to a node row; if caller wants to be strict
        // it must pass the resolved node_id. We only flag the obvious id
        // mismatch here.
        diff.immutable.push("target_node".into());
    }
    // target_dc has no stored-per-VM equivalent today; if set and a dc is
    // recorded on the node, the controller preflight already enforces it. We
    // treat it as immutable when provided.
    if !incoming.target_dc.is_empty() {
        // No stored field to compare to on VmRow; skip quietly.
    }

    // nics — v1 tracks only the primary network on VmRow.
    if let Some(nic) = incoming.spec.nics.first() {
        if !nic.network.is_empty() && stored.network != nic.network {
            diff.immutable.push("nics".into());
        }
    }

    // disks — VmRow has no per-disk detail (single boot disk today). If the
    // caller passes multiple disks, v1 can't represent that, so any explicit
    // list of >1 is flagged.
    if incoming.spec.disks.len() > 1 {
        diff.immutable.push("disks".into());
    }

    // ssh_key_names: compare as unordered sets.
    if !incoming.ssh_key_names.is_empty() || !stored_ssh_key_names.is_empty() {
        let mut a: Vec<&str> = stored_ssh_key_names.iter().map(|s| s.as_str()).collect();
        let mut b: Vec<&str> = incoming.ssh_key_names.iter().map(|s| s.as_str()).collect();
        a.sort();
        b.sort();
        if a != b {
            diff.immutable.push("ssh_key_names".into());
        }
    }

    diff
}

// ---------- Container ----------

pub struct ContainerApply<'a> {
    pub spec: &'a controller_proto::ContainerSpec,
    pub storage_backend: &'a str,
    pub storage_size_bytes: i64,
}

/// Extra container facts the diff needs but which don't live on `WorkloadRow`.
#[derive(Debug, Default, Clone)]
pub struct StoredContainerExtras {
    pub image: String,
    pub command: Vec<String>,
    pub env: HashMap<String, String>,
    pub ports: Vec<String>,
    pub mount_target: String,
}

pub fn diff_container(
    stored: &WorkloadRow,
    extras: &StoredContainerExtras,
    incoming: &ContainerApply<'_>,
) -> SpecDiff {
    let mut diff = SpecDiff::default();

    // Mutable ------------------------------------------------------------

    let incoming_desired =
        match controller_proto::WorkloadDesiredState::try_from(incoming.spec.desired_state)
            .unwrap_or(controller_proto::WorkloadDesiredState::Unspecified)
        {
            controller_proto::WorkloadDesiredState::Running => Some("running"),
            controller_proto::WorkloadDesiredState::Stopped => Some("stopped"),
            controller_proto::WorkloadDesiredState::Unspecified => None,
        };
    if let Some(want) = incoming_desired {
        if stored.desired_state != want {
            diff.mutable.push("desired_state".into());
        }
    }

    // Immutable ----------------------------------------------------------
    //
    // In v1 every spec field except `desired_state` is treated as immutable
    // because applying any of them requires recreating the container on the
    // node (env/ports included: containerd has no in-place mutate). The user
    // must explicitly delete and recreate the workload. Future versions can
    // promote env/ports to mutable by implementing a controlled rebuild.

    if !incoming.spec.image.is_empty() && extras.image != incoming.spec.image {
        diff.immutable.push("image".into());
    }
    // command / mount_target are not persisted on `WorkloadRow`. To avoid
    // false-positive immutable rejections on every re-apply, only diff them
    // when the caller actually tracks the stored value (i.e. extras is
    // non-empty). Once the DB starts persisting these, the diff just works.
    if !extras.command.is_empty()
        && !incoming.spec.command.is_empty()
        && extras.command != incoming.spec.command
    {
        diff.immutable.push("command".into());
    }
    if !incoming.spec.network.is_empty() && stored.network != incoming.spec.network {
        diff.immutable.push("network".into());
    }
    if !incoming.storage_backend.is_empty() && stored.storage_backend != incoming.storage_backend {
        diff.immutable.push("storage_backend".into());
    }
    if incoming.storage_size_bytes > 0 && stored.storage_size_bytes != incoming.storage_size_bytes {
        diff.immutable.push("storage_size_bytes".into());
    }
    if !extras.mount_target.is_empty()
        && !incoming.spec.mount_target.is_empty()
        && extras.mount_target != incoming.spec.mount_target
    {
        diff.immutable.push("mount_target".into());
    }

    // env/ports: if the caller tracked them in `extras` and they differ,
    // flag as immutable. If `extras` is empty (fresh stored row with no
    // durable env/ports), skip the check to avoid spurious rejections.
    if !extras.env.is_empty() && incoming.spec.env != extras.env {
        diff.immutable.push("env".into());
    }
    if !extras.ports.is_empty() {
        let mut stored_ports = extras.ports.clone();
        let mut incoming_ports = incoming.spec.ports.clone();
        stored_ports.sort();
        incoming_ports.sort();
        if stored_ports != incoming_ports {
            diff.immutable.push("ports".into());
        }
    }

    diff
}

// ---------- Network ----------

/// Incoming network apply (mirrors `CreateNetworkRequest` fields that belong
/// on `NetworkRow`).
pub struct NetworkApply<'a> {
    pub external_ip: &'a str,
    pub gateway_ip: &'a str,
    pub internal_netmask: &'a str,
    pub allowed_tcp_ports: Vec<i32>,
    pub allowed_udp_ports: Vec<i32>,
    pub vlan_id: i32,
    pub network_type: &'a str,
    pub enable_outbound_nat: bool,
}

/// In v1 every network field is immutable: rename/replace means delete.
pub fn diff_network(stored: &NetworkRow, incoming: &NetworkApply<'_>) -> SpecDiff {
    let mut diff = SpecDiff::default();

    if !incoming.external_ip.is_empty() && stored.external_ip != incoming.external_ip {
        diff.immutable.push("external_ip".into());
    }
    if !incoming.gateway_ip.is_empty() && stored.gateway_ip != incoming.gateway_ip {
        diff.immutable.push("gateway_ip".into());
    }
    if !incoming.internal_netmask.is_empty() && stored.internal_netmask != incoming.internal_netmask
    {
        diff.immutable.push("internal_netmask".into());
    }
    if !incoming.network_type.is_empty() && stored.network_type != incoming.network_type {
        diff.immutable.push("network_type".into());
    }
    if stored.vlan_id != incoming.vlan_id {
        diff.immutable.push("vlan_id".into());
    }
    if stored.enable_outbound_nat != incoming.enable_outbound_nat {
        diff.immutable.push("enable_outbound_nat".into());
    }

    let stored_tcp: Vec<i32> = parse_port_csv(&stored.allowed_tcp_ports);
    let stored_udp: Vec<i32> = parse_port_csv(&stored.allowed_udp_ports);
    let mut want_tcp = incoming.allowed_tcp_ports.clone();
    let mut want_udp = incoming.allowed_udp_ports.clone();
    let mut have_tcp = stored_tcp.clone();
    let mut have_udp = stored_udp.clone();
    want_tcp.sort();
    want_udp.sort();
    have_tcp.sort();
    have_udp.sort();
    if have_tcp != want_tcp {
        diff.immutable.push("allowed_tcp_ports".into());
    }
    if have_udp != want_udp {
        diff.immutable.push("allowed_udp_ports".into());
    }

    diff
}

fn parse_port_csv(csv: &str) -> Vec<i32> {
    csv.split(',')
        .filter_map(|p| {
            let p = p.trim();
            if p.is_empty() {
                None
            } else {
                p.parse::<i32>().ok()
            }
        })
        .collect()
}

// ---------- SshKey ----------

/// `public_key` is the only field; changing it is rejected (delete + recreate).
pub fn diff_ssh_key(stored_public_key: &str, incoming_public_key: &str) -> SpecDiff {
    let mut diff = SpecDiff::default();
    if stored_public_key.trim() != incoming_public_key.trim() {
        diff.immutable.push("public_key".into());
    }
    diff
}

// ---------- SecurityGroup ----------

pub struct SecurityGroupApply<'a> {
    pub description: &'a str,
    pub rules: &'a [controller_proto::SecurityGroupRule],
}

pub fn diff_security_group(
    stored: &SecurityGroupRow,
    stored_rules: &[SecurityGroupRuleRow],
    incoming: &SecurityGroupApply<'_>,
) -> SpecDiff {
    let mut diff = SpecDiff::default();

    if stored.description.trim() != incoming.description.trim() {
        diff.mutable.push("description".into());
    }

    if !rules_match(stored_rules, incoming.rules) {
        diff.mutable.push("rules".into());
    }

    diff
}

fn rules_match(
    stored: &[SecurityGroupRuleRow],
    incoming: &[controller_proto::SecurityGroupRule],
) -> bool {
    if stored.len() != incoming.len() {
        return false;
    }
    // Normalize protocol case on BOTH sides. The DB historically stored
    // whatever case the original CreateSecurityGroup payload used, while
    // the diff side already lower-cased the incoming spec — so a stored
    // "TCP" vs incoming "tcp" used to look like a real change and trigger
    // a spurious reconcile on every re-apply.
    let mut stored_keys: Vec<(String, i32, i32, String, String, bool)> = stored
        .iter()
        .map(|r| {
            (
                r.protocol.to_ascii_lowercase(),
                r.host_port,
                r.target_port,
                r.source_cidr.clone(),
                r.target_vm.clone(),
                r.enable_dnat,
            )
        })
        .collect();
    let mut incoming_keys: Vec<(String, i32, i32, String, String, bool)> = incoming
        .iter()
        .map(|r| {
            (
                r.protocol.to_ascii_lowercase(),
                r.host_port,
                if r.target_port <= 0 {
                    r.host_port
                } else {
                    r.target_port
                },
                r.source_cidr.clone(),
                r.target_vm.clone(),
                r.enable_dnat,
            )
        })
        .collect();
    stored_keys.sort();
    incoming_keys.sort();
    stored_keys == incoming_keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{NetworkRow, SecurityGroupRow, SecurityGroupRuleRow, VmRow, WorkloadRow};

    fn sample_vm() -> VmRow {
        VmRow {
            id: "vm-1".into(),
            name: "web".into(),
            cpu: 2,
            memory_bytes: 2048 * 1024 * 1024,
            image_path: "/var/lib/kcore/images/debian.qcow2".into(),
            image_url: "https://example.com/debian.qcow2".into(),
            image_sha256: "abcd".into(),
            image_format: "qcow2".into(),
            image_size: 8192,
            network: "default".into(),
            auto_start: true,
            node_id: "node-a".into(),
            created_at: String::new(),
            runtime_state: "running".into(),
            cloud_init_user_data: String::new(),
            storage_backend: "lvm".into(),
            storage_size_bytes: 20 * 1024 * 1024 * 1024,
            vm_ip: "10.0.0.5".into(),
        }
    }

    fn sample_vm_spec(cpu: i32, memory_bytes: i64) -> controller_proto::VmSpec {
        controller_proto::VmSpec {
            id: String::new(),
            name: "web".into(),
            cpu,
            memory_bytes,
            disks: Vec::new(),
            nics: Vec::new(),
            storage_backend: String::new(),
            storage_size_bytes: 0,
            desired_state: controller_proto::VmDesiredState::Unspecified as i32,
        }
    }

    #[test]
    fn vm_unchanged() {
        let stored = sample_vm();
        let spec = sample_vm_spec(stored.cpu, stored.memory_bytes);
        let apply = VmApply {
            spec: &spec,
            image_url: "",
            image_sha256: "",
            image_path: "",
            cloud_init_user_data: "",
            ssh_key_names: &[],
            storage_backend: "",
            storage_size_bytes: 0,
            target_node: "",
            target_dc: "",
        };
        let diff = diff_vm(&stored, &[], &apply);
        assert!(diff.is_unchanged(), "expected no diff, got {diff:?}");
    }

    #[test]
    fn vm_cpu_and_memory_change_is_mutable() {
        let stored = sample_vm();
        let spec = sample_vm_spec(stored.cpu + 2, stored.memory_bytes * 2);
        let apply = VmApply {
            spec: &spec,
            image_url: "",
            image_sha256: "",
            image_path: "",
            cloud_init_user_data: "",
            ssh_key_names: &[],
            storage_backend: "",
            storage_size_bytes: 0,
            target_node: "",
            target_dc: "",
        };
        let diff = diff_vm(&stored, &[], &apply);
        assert_eq!(diff.immutable, Vec::<String>::new());
        assert_eq!(
            diff.mutable,
            vec!["cpu".to_string(), "memory_bytes".to_string()]
        );
    }

    #[test]
    fn vm_desired_state_change_is_mutable() {
        let mut stored = sample_vm();
        stored.auto_start = true;
        let mut spec = sample_vm_spec(stored.cpu, stored.memory_bytes);
        spec.desired_state = controller_proto::VmDesiredState::Stopped as i32;
        let apply = VmApply {
            spec: &spec,
            image_url: "",
            image_sha256: "",
            image_path: "",
            cloud_init_user_data: "",
            ssh_key_names: &[],
            storage_backend: "",
            storage_size_bytes: 0,
            target_node: "",
            target_dc: "",
        };
        let diff = diff_vm(&stored, &[], &apply);
        assert_eq!(diff.mutable, vec!["desired_state".to_string()]);
        assert!(diff.immutable.is_empty());
    }

    #[test]
    fn vm_storage_backend_change_is_immutable() {
        let stored = sample_vm();
        let spec = sample_vm_spec(stored.cpu, stored.memory_bytes);
        let apply = VmApply {
            spec: &spec,
            image_url: "",
            image_sha256: "",
            image_path: "",
            cloud_init_user_data: "",
            ssh_key_names: &[],
            storage_backend: "zfs",
            storage_size_bytes: 0,
            target_node: "",
            target_dc: "",
        };
        let diff = diff_vm(&stored, &[], &apply);
        assert!(diff.immutable.iter().any(|f| f == "storage_backend"));
    }

    #[test]
    fn vm_ssh_keys_change_is_immutable() {
        let stored = sample_vm();
        let spec = sample_vm_spec(stored.cpu, stored.memory_bytes);
        let keys = vec!["alice".to_string()];
        let apply = VmApply {
            spec: &spec,
            image_url: "",
            image_sha256: "",
            image_path: "",
            cloud_init_user_data: "",
            ssh_key_names: &keys,
            storage_backend: "",
            storage_size_bytes: 0,
            target_node: "",
            target_dc: "",
        };
        let diff = diff_vm(&stored, &[], &apply);
        assert!(diff.immutable.iter().any(|f| f == "ssh_key_names"));
    }

    fn sample_container_row() -> WorkloadRow {
        WorkloadRow {
            id: "ctr-1".into(),
            name: "nginx".into(),
            kind: "container".into(),
            node_id: "node-a".into(),
            runtime_state: "running".into(),
            desired_state: "running".into(),
            vm_id: String::new(),
            container_image: "nginx:1".into(),
            network: "default".into(),
            storage_backend: "filesystem".into(),
            storage_size_bytes: 0,
            created_at: String::new(),
        }
    }

    fn sample_container_extras() -> StoredContainerExtras {
        StoredContainerExtras {
            image: "nginx:1".into(),
            command: Vec::new(),
            env: HashMap::new(),
            ports: vec!["80/tcp".into()],
            mount_target: "/data".into(),
        }
    }

    fn sample_container_spec() -> controller_proto::ContainerSpec {
        controller_proto::ContainerSpec {
            name: "nginx".into(),
            image: "nginx:1".into(),
            network: "default".into(),
            command: Vec::new(),
            env: HashMap::new(),
            ports: vec!["80/tcp".into()],
            storage_backend: "filesystem".into(),
            storage_size_bytes: 0,
            mount_target: "/data".into(),
            desired_state: controller_proto::WorkloadDesiredState::Unspecified as i32,
        }
    }

    #[test]
    fn container_unchanged() {
        let stored = sample_container_row();
        let extras = sample_container_extras();
        let spec = sample_container_spec();
        let apply = ContainerApply {
            spec: &spec,
            storage_backend: "filesystem",
            storage_size_bytes: 0,
        };
        let diff = diff_container(&stored, &extras, &apply);
        assert!(diff.is_unchanged(), "got {diff:?}");
    }

    #[test]
    fn container_env_change_is_immutable_when_stored() {
        let stored = sample_container_row();
        let mut extras = sample_container_extras();
        extras.env.insert("LOG".into(), "info".into());
        let mut spec = sample_container_spec();
        spec.env.insert("LOG".into(), "debug".into());
        let apply = ContainerApply {
            spec: &spec,
            storage_backend: "filesystem",
            storage_size_bytes: 0,
        };
        let diff = diff_container(&stored, &extras, &apply);
        assert!(diff.mutable.is_empty());
        assert_eq!(diff.immutable, vec!["env".to_string()]);
    }

    #[test]
    fn container_image_change_is_immutable() {
        let stored = sample_container_row();
        let extras = sample_container_extras();
        let mut spec = sample_container_spec();
        spec.image = "nginx:2".into();
        let apply = ContainerApply {
            spec: &spec,
            storage_backend: "filesystem",
            storage_size_bytes: 0,
        };
        let diff = diff_container(&stored, &extras, &apply);
        assert_eq!(diff.immutable, vec!["image".to_string()]);
    }

    #[test]
    fn container_command_and_mount_skip_diff_when_not_persisted() {
        // The controller currently does not persist command / mount_target
        // (extras is built with empty defaults). A re-apply that includes a
        // non-empty command + mount_target must NOT be flagged as immutable
        // — that would break idempotency for every real container manifest.
        let stored = sample_container_row();
        let extras = StoredContainerExtras {
            image: "nginx:1".into(),
            command: Vec::new(),
            env: HashMap::new(),
            ports: Vec::new(),
            mount_target: String::new(),
        };
        let mut spec = sample_container_spec();
        spec.command = vec!["nginx".into(), "-g".into(), "daemon off;".into()];
        spec.mount_target = "/data".into();
        let apply = ContainerApply {
            spec: &spec,
            storage_backend: "filesystem",
            storage_size_bytes: 0,
        };
        let diff = diff_container(&stored, &extras, &apply);
        assert!(diff.is_unchanged(), "got {diff:?}");
    }

    #[test]
    fn container_command_change_is_immutable_when_stored() {
        let stored = sample_container_row();
        let extras = StoredContainerExtras {
            image: "nginx:1".into(),
            command: vec!["nginx".into()],
            env: HashMap::new(),
            ports: Vec::new(),
            mount_target: String::new(),
        };
        let mut spec = sample_container_spec();
        spec.command = vec!["sh".into()];
        let apply = ContainerApply {
            spec: &spec,
            storage_backend: "filesystem",
            storage_size_bytes: 0,
        };
        let diff = diff_container(&stored, &extras, &apply);
        assert_eq!(diff.immutable, vec!["command".to_string()]);
    }

    #[test]
    fn container_desired_state_change_is_mutable() {
        let stored = sample_container_row();
        let extras = sample_container_extras();
        let mut spec = sample_container_spec();
        spec.desired_state = controller_proto::WorkloadDesiredState::Stopped as i32;
        let apply = ContainerApply {
            spec: &spec,
            storage_backend: "filesystem",
            storage_size_bytes: 0,
        };
        let diff = diff_container(&stored, &extras, &apply);
        assert_eq!(diff.mutable, vec!["desired_state".to_string()]);
    }

    fn sample_network_row() -> NetworkRow {
        NetworkRow {
            name: "web".into(),
            external_ip: "192.168.1.10".into(),
            gateway_ip: "10.10.0.1".into(),
            internal_netmask: "255.255.255.0".into(),
            node_id: "node-a".into(),
            allowed_tcp_ports: "80,443".into(),
            allowed_udp_ports: String::new(),
            vlan_id: 0,
            network_type: "nat".into(),
            enable_outbound_nat: true,
            vni: 0,
            next_ip: 2,
        }
    }

    #[test]
    fn network_unchanged() {
        let stored = sample_network_row();
        let apply = NetworkApply {
            external_ip: &stored.external_ip.clone(),
            gateway_ip: &stored.gateway_ip.clone(),
            internal_netmask: &stored.internal_netmask.clone(),
            allowed_tcp_ports: vec![80, 443],
            allowed_udp_ports: Vec::new(),
            vlan_id: 0,
            network_type: "nat",
            enable_outbound_nat: true,
        };
        let diff = diff_network(&stored, &apply);
        assert!(diff.is_unchanged(), "got {diff:?}");
    }

    #[test]
    fn network_change_is_immutable() {
        let stored = sample_network_row();
        let apply = NetworkApply {
            external_ip: "192.168.1.11",
            gateway_ip: &stored.gateway_ip.clone(),
            internal_netmask: &stored.internal_netmask.clone(),
            allowed_tcp_ports: vec![80],
            allowed_udp_ports: Vec::new(),
            vlan_id: 0,
            network_type: "nat",
            enable_outbound_nat: true,
        };
        let diff = diff_network(&stored, &apply);
        assert!(diff.mutable.is_empty());
        assert!(diff.immutable.contains(&"external_ip".to_string()));
        assert!(diff.immutable.contains(&"allowed_tcp_ports".to_string()));
    }

    #[test]
    fn ssh_key_unchanged() {
        let diff = diff_ssh_key("ssh-ed25519 AAA alice", "ssh-ed25519 AAA alice");
        assert!(diff.is_unchanged());
    }

    #[test]
    fn ssh_key_change_is_immutable() {
        let diff = diff_ssh_key("ssh-ed25519 AAA alice", "ssh-ed25519 BBB alice");
        assert_eq!(diff.immutable, vec!["public_key".to_string()]);
    }

    fn sample_sg_row() -> SecurityGroupRow {
        SecurityGroupRow {
            name: "web".into(),
            description: "allow http".into(),
            created_at: String::new(),
        }
    }

    fn sample_sg_rule() -> SecurityGroupRuleRow {
        SecurityGroupRuleRow {
            id: "r1".into(),
            security_group: "web".into(),
            protocol: "tcp".into(),
            host_port: 80,
            target_port: 80,
            source_cidr: String::new(),
            target_vm: String::new(),
            enable_dnat: false,
        }
    }

    fn sample_sg_proto_rule() -> controller_proto::SecurityGroupRule {
        controller_proto::SecurityGroupRule {
            id: "r1".into(),
            protocol: "tcp".into(),
            host_port: 80,
            target_port: 80,
            source_cidr: String::new(),
            target_vm: String::new(),
            enable_dnat: false,
        }
    }

    #[test]
    fn sg_unchanged() {
        let row = sample_sg_row();
        let rules = vec![sample_sg_rule()];
        let incoming_rules = vec![sample_sg_proto_rule()];
        let apply = SecurityGroupApply {
            description: "allow http",
            rules: &incoming_rules,
        };
        let diff = diff_security_group(&row, &rules, &apply);
        assert!(diff.is_unchanged(), "got {diff:?}");
    }

    #[test]
    fn sg_description_change_is_mutable() {
        let row = sample_sg_row();
        let rules = vec![sample_sg_rule()];
        let incoming_rules = vec![sample_sg_proto_rule()];
        let apply = SecurityGroupApply {
            description: "allow http v2",
            rules: &incoming_rules,
        };
        let diff = diff_security_group(&row, &rules, &apply);
        assert_eq!(diff.mutable, vec!["description".to_string()]);
    }

    #[test]
    fn sg_rules_change_is_mutable() {
        let row = sample_sg_row();
        let rules = vec![sample_sg_rule()];
        let mut changed = sample_sg_proto_rule();
        changed.host_port = 8080;
        let incoming_rules = vec![changed];
        let apply = SecurityGroupApply {
            description: "allow http",
            rules: &incoming_rules,
        };
        let diff = diff_security_group(&row, &rules, &apply);
        assert!(diff.mutable.contains(&"rules".to_string()));
    }

    #[test]
    fn sg_rules_protocol_case_is_normalized_on_both_sides() {
        // Regression: the diff used to lowercase the *incoming* protocol
        // but compare it against the raw (possibly upper-case) stored
        // protocol. Re-applying a manifest with `protocol: tcp` against
        // a row that historically stored `TCP` reported a spurious
        // `rules` mutation on every apply.
        let row = sample_sg_row();
        let mut stored = sample_sg_rule();
        stored.protocol = "TCP".into();
        let rules = vec![stored];
        let mut incoming = sample_sg_proto_rule();
        incoming.protocol = "tcp".into();
        let incoming_rules = vec![incoming];
        let apply = SecurityGroupApply {
            description: "allow http",
            rules: &incoming_rules,
        };
        let diff = diff_security_group(&row, &rules, &apply);
        assert!(
            diff.is_unchanged(),
            "TCP vs tcp must compare equal, got {diff:?}"
        );
    }
}
