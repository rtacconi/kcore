use crate::controller_proto;
use crate::node_proto;

pub fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn short_vm_id_seed() -> String {
    let raw = uuid_v4();
    let start = raw.len().saturating_sub(8);
    raw[start..].to_string()
}

pub fn controller_state_from_node_state(state: i32) -> i32 {
    match node_proto::VmState::try_from(state).unwrap_or(node_proto::VmState::Unknown) {
        node_proto::VmState::Unknown => controller_proto::VmState::Unknown as i32,
        node_proto::VmState::Stopped => controller_proto::VmState::Stopped as i32,
        node_proto::VmState::Running => controller_proto::VmState::Running as i32,
        node_proto::VmState::Paused => controller_proto::VmState::Paused as i32,
        node_proto::VmState::Error => controller_proto::VmState::Error as i32,
    }
}

pub fn state_fallback_without_runtime(auto_start: bool) -> i32 {
    if auto_start {
        controller_proto::VmState::Unknown as i32
    } else {
        controller_proto::VmState::Stopped as i32
    }
}

pub fn parse_datetime_to_timestamp(dt: &str) -> Option<prost_types::Timestamp> {
    let parts: Vec<&str> = dt.split(['-', ' ', ':']).collect();
    if parts.len() < 6 {
        return None;
    }
    let year: i32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;
    let hour: u32 = parts[3].parse().ok()?;
    let min: u32 = parts[4].parse().ok()?;
    let sec: u32 = parts[5].parse().ok()?;

    let days = days_from_civil(year, month, day);
    let secs = days * 86400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64;
    Some(prost_types::Timestamp {
        seconds: secs,
        nanos: 0,
    })
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = m as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

pub fn parse_port_list(s: &str) -> Vec<i32> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(',').filter_map(|p| p.trim().parse().ok()).collect()
}

/// Derive the actual disk backend handle (path or device) for a VM.
/// LVM  → `/dev/vg_kcore/kcore-<name>`
/// ZFS  → `/dev/zvol/tank0/kcore-<name>`
/// else → cached image file path
pub fn vm_backend_handle(vm: &crate::db::VmRow) -> String {
    let sanitized = || -> String {
        vm.name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    };
    match vm.storage_backend.as_str() {
        "lvm" => format!("/dev/vg_kcore/kcore-{}", sanitized()),
        "zfs" => format!("/dev/zvol/tank0/kcore-{}", sanitized()),
        _ => vm.image_path.clone(),
    }
}

/// Deterministic VNI from network name. Range 10000–15999.
pub fn compute_vni(name: &str) -> i32 {
    let mut hash: u32 = 5381;
    for b in name.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u32);
    }
    10000 + (hash % 6000) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_vni_stays_in_range() {
        for name in ["overlay", "prod", "test-vxlan", "a", "zzzzz"] {
            let vni = compute_vni(name);
            assert!(vni >= 10000, "vni {vni} below 10000 for '{name}'");
            assert!(vni < 16000, "vni {vni} above 15999 for '{name}'");
        }
    }

    #[test]
    fn compute_vni_is_deterministic() {
        assert_eq!(compute_vni("overlay"), compute_vni("overlay"));
    }

    fn test_vm_row(name: &str, backend: &str) -> crate::db::VmRow {
        crate::db::VmRow {
            id: "test-id".to_string(),
            name: name.to_string(),
            cpu: 1,
            memory_bytes: 1024 * 1024 * 1024,
            image_path: "/var/lib/kcore/images/test.qcow2".to_string(),
            image_url: String::new(),
            image_sha256: String::new(),
            image_format: "qcow2".to_string(),
            image_size: 0,
            network: "default".to_string(),
            auto_start: true,
            node_id: "node-1".to_string(),
            created_at: String::new(),
            runtime_state: "running".to_string(),
            cloud_init_user_data: String::new(),
            storage_backend: backend.to_string(),
            storage_size_bytes: 3221225472,
            vm_ip: String::new(),
        }
    }

    #[test]
    fn vm_backend_handle_lvm() {
        let vm = test_vm_row("my-vm", "lvm");
        assert_eq!(vm_backend_handle(&vm), "/dev/vg_kcore/kcore-my-vm");
    }

    #[test]
    fn vm_backend_handle_zfs() {
        let vm = test_vm_row("my-vm", "zfs");
        assert_eq!(vm_backend_handle(&vm), "/dev/zvol/tank0/kcore-my-vm");
    }

    #[test]
    fn vm_backend_handle_filesystem() {
        let vm = test_vm_row("my-vm", "filesystem");
        assert_eq!(vm_backend_handle(&vm), "/var/lib/kcore/images/test.qcow2");
    }
}

/// Property-based tests (Phase 2) for the small pure helpers.
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// `compute_vni` must always land in the documented `[10000, 16000)`
        /// range, for **any** input string. The previous unit tests only
        /// covered five hard-coded names.
        #[test]
        fn compute_vni_stays_in_range(name in ".{0,64}") {
            let vni = compute_vni(&name);
            prop_assert!(vni >= 10000);
            prop_assert!(vni < 16000);
        }

        /// `compute_vni` is deterministic — same input ⇒ same output.
        /// A regression here would silently rotate VNIs after a process
        /// restart and break VXLAN connectivity.
        #[test]
        fn compute_vni_is_deterministic(name in ".{0,64}") {
            prop_assert_eq!(compute_vni(&name), compute_vni(&name));
        }

        /// `parse_port_list` never panics, returns an empty vec for an
        /// empty input, and only ever yields integer-parseable tokens.
        #[test]
        fn parse_port_list_only_returns_parseable_integers(s in ".{0,64}") {
            let ports = parse_port_list(&s);
            for p in &ports {
                // The output must round-trip through `i32::to_string` /
                // `parse` (this is the contract callers rely on).
                let printed = p.to_string();
                let _: i32 = printed.parse().expect("output must be parseable");
            }
            if s.is_empty() {
                prop_assert!(ports.is_empty());
            }
        }

        /// `parse_port_list` order is preserved (matches the order tokens
        /// appear in the comma-separated list, modulo dropped invalid
        /// tokens). We verify by independently building the expected list.
        #[test]
        fn parse_port_list_preserves_order(
            ports in proptest::collection::vec(0i32..=65_535, 0..6),
        ) {
            let s = ports
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let parsed = parse_port_list(&s);
            prop_assert_eq!(parsed, ports);
        }

        /// `controller_state_from_node_state` must:
        ///   1. Never panic, regardless of i32 input (including bogus
        ///      enum values).
        ///   2. Always produce a value that maps back to a valid
        ///      `controller_proto::VmState` enum entry.
        #[test]
        fn controller_state_from_node_state_always_known(state in any::<i32>()) {
            let s = controller_state_from_node_state(state);
            // Must round-trip into `VmState`.
            let _ = controller_proto::VmState::try_from(s)
                .expect("controller_state_from_node_state must produce a known enum");
        }

        /// `state_fallback_without_runtime` is a tiny boolean partition
        /// with documented mapping; encode that as a property.
        #[test]
        fn state_fallback_without_runtime_partitions_on_auto_start(auto_start in any::<bool>()) {
            let got = state_fallback_without_runtime(auto_start);
            let expected = if auto_start {
                controller_proto::VmState::Unknown as i32
            } else {
                controller_proto::VmState::Stopped as i32
            };
            prop_assert_eq!(got, expected);
        }

        /// `short_vm_id_seed` always returns at most 8 bytes (the
        /// "short" suffix of a UUID).
        #[test]
        fn short_vm_id_seed_is_at_most_eight_bytes(_seed in any::<u8>()) {
            let s = short_vm_id_seed();
            prop_assert!(s.len() <= 8);
        }
    }
}
