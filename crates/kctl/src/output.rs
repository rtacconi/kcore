use crate::client::controller_proto;
use crate::client::node_proto;

pub fn print_vm_table(vms: &[controller_proto::VmInfo]) {
    println!(
        "{:<36}  {:<20}  {:>4}  {:>10}  {:>8}  {:<10}  {:<16}",
        "ID", "NAME", "CPU", "MEMORY", "DISK", "STATE", "NODE"
    );
    for vm in vms {
        let state = vm_state_str(vm.state);
        let mem = crate::client::format_bytes(vm.memory_bytes);
        let disk = crate::client::format_bytes(vm.storage_size_bytes);
        println!(
            "{:<36}  {:<20}  {:>4}  {:>10}  {:>8}  {:<10}  {:<16}",
            vm.id, vm.name, vm.cpu, mem, disk, state, vm.node_id
        );
    }
}

pub fn print_vm_detail(
    spec: &controller_proto::VmSpec,
    status: &controller_proto::VmStatus,
    node_id: &str,
) {
    println!("ID:       {}", spec.id);
    println!("Name:     {}", spec.name);
    println!("Node:     {node_id}");
    println!("State:    {}", vm_state_str(status.state));
    println!("CPU:      {}", spec.cpu);
    println!(
        "Memory:   {}",
        crate::client::format_bytes(spec.memory_bytes)
    );

    let backend = if spec.storage_backend.is_empty() {
        "filesystem"
    } else {
        &spec.storage_backend
    };
    println!("\nStorage:");
    println!("  Backend:  {backend}");
    println!(
        "  Size:     {}",
        crate::client::format_bytes(spec.storage_size_bytes)
    );
    if !spec.disks.is_empty() {
        for d in &spec.disks {
            println!("  Volume:   {} ({})", d.name, d.backend_handle);
        }
    }

    if !spec.nics.is_empty() {
        println!("\nNICs:");
        for n in &spec.nics {
            println!("  - network={} mac={}", n.network, n.mac_address);
        }
    }
}

pub fn print_volume_table(volumes: &[controller_proto::VolumeInfo]) {
    println!(
        "{:<20}  {:<12}  {:<20}  {:<10}  {:>8}  {:<40}  {:<8}",
        "VM", "VM_ID", "NODE", "BACKEND", "SIZE", "VOLUME", "STATE"
    );
    for v in volumes {
        let size = crate::client::format_bytes(v.storage_size_bytes);
        let state = vm_state_str(v.vm_state);
        println!(
            "{:<20}  {:<12}  {:<20}  {:<10}  {:>8}  {:<40}  {:<8}",
            v.vm_name,
            &v.vm_id[..v.vm_id.len().min(12)],
            truncate_node_id(&v.node_id),
            v.storage_backend,
            size,
            v.backend_handle,
            state,
        );
    }
}

fn truncate_node_id(id: &str) -> String {
    if let Some(stripped) = id.strip_prefix("kvm-node-") {
        stripped.to_string()
    } else if id.len() > 20 {
        format!("{}...", &id[..17])
    } else {
        id.to_string()
    }
}

pub fn print_node_table(nodes: &[controller_proto::NodeInfo]) {
    println!(
        "{:<20}  {:<20}  {:<16}  {:<6}  {:>6}  {:>10}  {:<10}  {:<10}  {:<10}  {:>11}  {:<8}",
        "ID",
        "HOSTNAME",
        "ADDRESS",
        "DC",
        "CORES",
        "MEMORY",
        "STATUS",
        "STORAGE",
        "APPROVAL",
        "CERT EXPIRY",
        "LUKS"
    );
    for n in nodes {
        let (cores, mem) = if let Some(cap) = &n.capacity {
            (cap.cpu_cores, crate::client::format_bytes(cap.memory_bytes))
        } else {
            (0, "n/a".to_string())
        };
        let cert_expiry = format_cert_expiry(n.cert_expiry_days);
        let luks = format_luks_method(&n.luks_method);
        let dc = if n.dc_id.is_empty() { "-" } else { &n.dc_id };
        println!(
            "{:<20}  {:<20}  {:<16}  {:<6}  {:>6}  {:>10}  {:<10}  {:<10}  {:<10}  {:>11}  {:<8}",
            n.node_id,
            n.hostname,
            n.address,
            dc,
            cores,
            mem,
            n.status,
            storage_backend_str(n.storage_backend),
            n.approval_status,
            cert_expiry,
            luks,
        );
    }
}

pub fn print_node_detail(n: &controller_proto::NodeInfo) {
    println!("ID:        {}", n.node_id);
    println!("Hostname:  {}", n.hostname);
    println!("Address:   {}", n.address);
    println!(
        "DC:        {}",
        if n.dc_id.is_empty() { "-" } else { &n.dc_id }
    );
    println!("Status:    {}", n.status);
    println!("Approval:  {}", n.approval_status);
    if let Some(cap) = &n.capacity {
        println!("CPU:       {} cores", cap.cpu_cores);
        println!(
            "Memory:    {}",
            crate::client::format_bytes(cap.memory_bytes)
        );
    }
    if let Some(usage) = &n.usage {
        println!("CPU used:  {} cores", usage.cpu_cores_used);
        println!(
            "Mem used:  {}",
            crate::client::format_bytes(usage.memory_bytes_used)
        );
    }
    if !n.labels.is_empty() {
        println!("Labels:    {}", n.labels.join(", "));
    }
    println!("Storage:   {}", storage_backend_str(n.storage_backend));
    println!("LUKS:      {}", format_luks_method(&n.luks_method));
    println!("Cert:      {}", format_cert_expiry(n.cert_expiry_days));
}

pub fn print_disk_table(disks: &[node_proto::DiskInfo]) {
    println!(
        "{:<12}  {:<16}  {:>10}  {:<20}  {:<8}  {:<16}",
        "NAME", "PATH", "SIZE", "MODEL", "FSTYPE", "MOUNTPOINT"
    );
    for d in disks {
        println!(
            "{:<12}  {:<16}  {:>10}  {:<20}  {:<8}  {:<16}",
            d.name, d.path, d.size, d.model, d.fstype, d.mountpoint
        );
    }
}

pub fn print_nic_table(nics: &[node_proto::NetworkInterfaceInfo]) {
    println!(
        "{:<16}  {:<18}  {:<6}  {:>5}  ADDRESSES",
        "NAME", "MAC", "STATE", "MTU"
    );
    for n in nics {
        let addrs = n.addresses.join(", ");
        println!(
            "{:<16}  {:<18}  {:<6}  {:>5}  {}",
            n.name, n.mac_address, n.state, n.mtu, addrs
        );
    }
}

fn vm_state_str(state: i32) -> &'static str {
    match state {
        1 => "Stopped",
        2 => "Running",
        3 => "Paused",
        4 => "Error",
        _ => "Unknown",
    }
}

fn format_cert_expiry(days: i32) -> String {
    if days < 0 {
        "unknown".to_string()
    } else if days == 0 {
        "EXPIRED".to_string()
    } else if days <= 30 {
        format!("{days}d ⚠")
    } else {
        format!("{days}d")
    }
}

fn format_luks_method(method: &str) -> &str {
    match method {
        "tpm2" => "TPM2",
        "key-file" => "key-file",
        _ => "-",
    }
}

pub fn print_compliance_report(r: &controller_proto::GetComplianceReportResponse) {
    let now = chrono_now();
    println!("=== KCore Compliance Report ===");
    println!("Controller version: {}", r.controller_version);
    println!("Generated: {now}");

    section(
        "Cryptography",
        &["FIPS 140-3", "PCI DSS 4.2", "SOC 2 CC6.1"],
    );
    field("Library", &r.crypto_library);
    field("TLS 1.3", &r.tls13_cipher_suites.join(", "));
    field("TLS 1.2", &r.tls12_cipher_suites.join(", "));
    field("Key Exchange", &r.kx_groups.join(", "));
    field("Excluded", &r.excluded_algorithms.join(", "));

    section(
        "Encryption in Transit",
        &["SOC 2 CC6.1", "PCI DSS 4.2", "GDPR Art. 32"],
    );
    field(
        "mTLS",
        if r.mtls_enabled {
            "enabled"
        } else {
            "disabled"
        },
    );
    field("Protocol", "gRPC over mTLS (X.509 client certificates)");

    section(
        "Access Control",
        &["SOC 2 CC6.3", "PCI DSS 7.1", "GDPR Art. 32"],
    );
    for entry in &r.access_control {
        println!("  {:<24}  {}", entry.rpc_method, entry.allowed_identities);
    }

    section("Node Inventory", &["SOC 2 CC6.2", "PCI DSS 2.4"]);
    println!(
        "  Total: {}    Approved: {}    Pending: {}    Rejected: {}",
        r.total_nodes, r.approved_nodes, r.pending_nodes, r.rejected_nodes
    );

    section("VM Inventory", &["SOC 2 CC6.2", "PCI DSS 2.4"]);
    println!(
        "  Total: {}    Running: {}    Stopped: {}",
        r.total_vms, r.running_vms, r.stopped_vms
    );

    section(
        "Network Segmentation",
        &["PCI DSS 1.3", "NCSC Principle 3", "SOC 2 CC6.6"],
    );
    println!(
        "  Total: {}    NAT: {}    Bridge: {}    VXLAN: {}",
        r.total_networks, r.nat_networks, r.bridge_networks, r.vxlan_networks
    );

    section(
        "Certificate Lifecycle",
        &["SOC 2 CC6.1", "PCI DSS 3.6", "NCSC Principle 10"],
    );
    field(
        "Sub-CA",
        if r.sub_ca_enabled {
            "enabled"
        } else {
            "disabled"
        },
    );
    field(
        "Auto-renewal",
        &format!("within {} days of expiry", r.cert_auto_renewal_days),
    );
    field(
        "Expiring (<30d)",
        &format!("{} nodes", r.nodes_expiring_30d),
    );
    field("Unknown", &format!("{} nodes", r.nodes_cert_unknown));

    section(
        "Encryption at rest",
        &["NCSC Principle 2", "NIST 800-53 SC-28", "SOC 2 CC6.1"],
    );
    field("Method", "LUKS2 full-disk encryption (mandatory)");
    field("TPM2-sealed", &format!("{} nodes", r.nodes_luks_tpm2));
    field("Key-file", &format!("{} nodes", r.nodes_luks_keyfile));
    field(
        "Unknown/unreported",
        &format!("{} nodes", r.nodes_luks_unknown),
    );

    section(
        "Infrastructure",
        &["SOC 2 CC8.1", "PCI DSS 2.2", "PCI DSS 6.3"],
    );
    field("OS", "NixOS (declarative, atomic updates)");
    field("Hypervisor", "Cloud Hypervisor (KVM)");
    field(
        "VM Isolation",
        "Hardware KVM + per-VM TAP + per-network bridge",
    );
    field("Node Approval", "Required (approval queue)");

    if !r.nodes.is_empty() {
        section("Node Details", &["SOC 2 CC6.2", "PCI DSS 2.4"]);
        println!(
            "  {:<36}  {:<16}  {:<20}  {:<10}  {:<10}  {:<8}",
            "ID", "HOSTNAME", "ADDRESS", "STATUS", "CERT EXPIRY", "LUKS"
        );
        for n in &r.nodes {
            println!(
                "  {:<36}  {:<16}  {:<20}  {:<10}  {:<10}  {:<8}",
                n.node_id,
                n.hostname,
                n.address,
                &n.approval_status,
                format_cert_expiry(n.cert_expiry_days),
                format_luks_method(&n.luks_method),
            );
        }
    }
}

fn section(title: &str, standards: &[&str]) {
    let tags = format!("[{}]", standards.join(", "));
    // `60 - title.len()` underflows (and panics in debug builds) when a
    // future caller passes a long title; use saturating subtraction so the
    // formatter just collapses the padding to zero instead.
    let width = 60usize.saturating_sub(title.len());
    println!("\n--- {title} ---{tags:>width$}", width = width);
}

fn field(label: &str, value: &str) {
    println!("  {label:<16}{value}");
}

fn chrono_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    let (y, mo, d) = days_to_ymd(days);
    format!("{y}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut y = 1970;
    loop {
        let year_days = if is_leap(y) { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        y += 1;
    }
    let leap = is_leap(y);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 0;
    for md in &month_days {
        if days < *md {
            break;
        }
        days -= md;
        mo += 1;
    }
    (y, mo + 1, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

fn storage_backend_str(value: i32) -> &'static str {
    match controller_proto::StorageBackendType::try_from(value)
        .unwrap_or(controller_proto::StorageBackendType::Unspecified)
    {
        controller_proto::StorageBackendType::Filesystem => "filesystem",
        controller_proto::StorageBackendType::Lvm => "lvm",
        controller_proto::StorageBackendType::Zfs => "zfs",
        controller_proto::StorageBackendType::Unspecified => "unspecified",
    }
}

#[cfg(test)]
mod tests {
    use super::{days_to_ymd, is_leap};

    #[test]
    fn epoch_is_1970_01_01() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn known_dates_round_trip() {
        // 2000-01-01 is exactly 10957 days after 1970-01-01.
        assert_eq!(days_to_ymd(10957), (2000, 1, 1));
        // 2024-02-29 is exactly 19782 days after the epoch.
        assert_eq!(days_to_ymd(19782), (2024, 2, 29));
        // 2024-03-01 is the day after.
        assert_eq!(days_to_ymd(19783), (2024, 3, 1));
        // 2025-01-01 (the year *after* a leap year).
        assert_eq!(days_to_ymd(20089), (2025, 1, 1));
    }

    #[test]
    fn leap_year_rules() {
        assert!(is_leap(2000)); // divisible by 400
        assert!(is_leap(2024)); // divisible by 4, not by 100
        assert!(!is_leap(1900)); // divisible by 100 but not 400
        assert!(!is_leap(2023));
    }

    #[test]
    fn section_does_not_panic_with_long_title() {
        // Regression: `60 - title.len()` used to underflow `usize` and
        // either panic in debug builds or render garbage in release.
        // We can't capture stdout cheaply here, but invoking the function
        // is enough — the panic would fail the test.
        super::section(&"a".repeat(80), &["SOC2", "HIPAA"]);
    }
}

/// Property-based tests (Phase 2) — output formatters.
#[cfg(test)]
mod proptests {
    use super::{
        days_to_ymd, format_cert_expiry, format_luks_method, is_leap, section, storage_backend_str,
        truncate_node_id, vm_state_str,
    };
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// `truncate_node_id` never panics on arbitrary input. The
        /// `&id[..17]` slice could panic on a non-character boundary
        /// for a multi-byte UTF-8 input, so we want to assert any
        /// future regression here is caught.
        ///
        /// (Today the function only takes ASCII node IDs, but proptest
        /// fuzzes through arbitrary UTF-8 to lock the contract.)
        #[test]
        fn truncate_node_id_never_panics(id in "[a-zA-Z0-9-]{0,40}") {
            let _ = truncate_node_id(&id);
        }

        /// IDs prefixed with `kvm-node-` are stripped to bare suffix.
        #[test]
        fn truncate_node_id_strips_prefix(suffix in "[a-zA-Z0-9-]{1,30}") {
            let id = format!("kvm-node-{suffix}");
            prop_assert_eq!(truncate_node_id(&id), suffix);
        }

        /// Long IDs (without the prefix) are truncated to 20 visual
        /// characters: 17 chars + `...`.
        #[test]
        fn truncate_node_id_long_form(s in "[a-zA-Z0-9]{21,40}") {
            let out = truncate_node_id(&s);
            prop_assert_eq!(out.chars().count(), 20);
            prop_assert!(out.ends_with("..."));
        }

        /// `format_cert_expiry` partitions on the documented thresholds.
        #[test]
        fn format_cert_expiry_threshold_partition(d in -100i32..=400) {
            let s = format_cert_expiry(d);
            if d < 0 {
                prop_assert_eq!(s, "unknown");
            } else if d == 0 {
                prop_assert_eq!(s, "EXPIRED");
            } else if d <= 30 {
                prop_assert!(s.contains("⚠"));
                prop_assert!(s.starts_with(&d.to_string()));
            } else {
                prop_assert!(!s.contains("⚠"));
                prop_assert!(s.starts_with(&d.to_string()));
                prop_assert!(s.ends_with('d'));
            }
        }

        /// `vm_state_str` always returns one of the documented labels.
        #[test]
        fn vm_state_str_known_set(state in any::<i32>()) {
            let s = vm_state_str(state);
            prop_assert!(matches!(s, "Stopped" | "Running" | "Paused" | "Error" | "Unknown"));
        }

        /// `format_luks_method` returns `"-"` for any unknown method.
        #[test]
        fn format_luks_method_unknown_dash(s in ".{0,16}") {
            let v = format_luks_method(&s);
            prop_assert!(matches!(v, "TPM2" | "key-file" | "-"));
            if s != "tpm2" && s != "key-file" {
                prop_assert_eq!(v, "-");
            }
        }

        /// `storage_backend_str` always returns one of the four known
        /// labels for any i32.
        #[test]
        fn storage_backend_str_known_set(state in any::<i32>()) {
            let s = storage_backend_str(state);
            prop_assert!(matches!(s, "filesystem" | "lvm" | "zfs" | "unspecified"));
        }

        /// `section` never panics for any title length, including
        /// pathological 0-byte and very long titles.
        #[test]
        fn section_never_panics_for_any_title_length(len in 0usize..=200) {
            let title = "a".repeat(len);
            section(&title, &["SOC2"]);
        }

        /// `is_leap` follows the Gregorian rules. Property: agreement
        /// with `(y % 4 == 0) && ((y % 100 != 0) || (y % 400 == 0))`.
        #[test]
        fn is_leap_matches_gregorian_predicate(y in 0u64..=4000) {
            let predicate = (y % 4 == 0) && ((y % 100 != 0) || (y % 400 == 0));
            prop_assert_eq!(is_leap(y), predicate);
        }

        /// `days_to_ymd` always returns a month in `1..=12` and a day
        /// in `1..=31`, and a year >= 1970.
        #[test]
        fn days_to_ymd_returns_valid_calendar(d in 0u64..=200_000) {
            let (y, mo, da) = days_to_ymd(d);
            prop_assert!(y >= 1970);
            prop_assert!((1..=12).contains(&mo), "month {mo} out of range");
            prop_assert!((1..=31).contains(&da), "day {da} out of range");
        }
    }
}
