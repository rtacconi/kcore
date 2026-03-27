use crate::client::controller_proto;
use crate::client::node_proto;

pub fn print_vm_table(vms: &[controller_proto::VmInfo]) {
    println!(
        "{:<36}  {:<20}  {:>4}  {:>10}  {:<10}  {:<16}",
        "ID", "NAME", "CPU", "MEMORY", "STATE", "NODE"
    );
    for vm in vms {
        let state = vm_state_str(vm.state);
        let mem = crate::client::format_bytes(vm.memory_bytes);
        println!(
            "{:<36}  {:<20}  {:>4}  {:>10}  {:<10}  {:<16}",
            vm.id, vm.name, vm.cpu, mem, state, vm.node_id
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

    if !spec.disks.is_empty() {
        println!("Disks:");
        for d in &spec.disks {
            println!("  - {} ({})", d.name, d.backend_handle);
        }
    }

    if !spec.nics.is_empty() {
        println!("NICs:");
        for n in &spec.nics {
            println!("  - network={} mac={}", n.network, n.mac_address);
        }
    }
}

pub fn print_node_table(nodes: &[controller_proto::NodeInfo]) {
    println!(
        "{:<20}  {:<20}  {:<16}  {:>6}  {:>10}  {:<10}  {:<10}  {:<10}  {:>11}",
        "ID", "HOSTNAME", "ADDRESS", "CORES", "MEMORY", "STATUS", "STORAGE", "APPROVAL", "CERT EXPIRY"
    );
    for n in nodes {
        let (cores, mem) = if let Some(cap) = &n.capacity {
            (cap.cpu_cores, crate::client::format_bytes(cap.memory_bytes))
        } else {
            (0, "n/a".to_string())
        };
        let cert_expiry = format_cert_expiry(n.cert_expiry_days);
        println!(
            "{:<20}  {:<20}  {:<16}  {:>6}  {:>10}  {:<10}  {:<10}  {:<10}  {:>11}",
            n.node_id,
            n.hostname,
            n.address,
            cores,
            mem,
            n.status,
            storage_backend_str(n.storage_backend),
            n.approval_status,
            cert_expiry,
        );
    }
}

pub fn print_node_detail(n: &controller_proto::NodeInfo) {
    println!("ID:        {}", n.node_id);
    println!("Hostname:  {}", n.hostname);
    println!("Address:   {}", n.address);
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
        if r.mtls_enabled { "enabled" } else { "disabled" },
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
}

fn section(title: &str, standards: &[&str]) {
    let tags = format!("[{}]", standards.join(", "));
    println!("\n--- {title} ---{tags:>width$}", width = 60 - title.len());
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
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
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
