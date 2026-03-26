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
        "{:<20}  {:<20}  {:<16}  {:>6}  {:>10}  {:<10}  {:<10}",
        "ID", "HOSTNAME", "ADDRESS", "CORES", "MEMORY", "STATUS", "STORAGE"
    );
    for n in nodes {
        let (cores, mem) = if let Some(cap) = &n.capacity {
            (cap.cpu_cores, crate::client::format_bytes(cap.memory_bytes))
        } else {
            (0, "n/a".to_string())
        };
        println!(
            "{:<20}  {:<20}  {:<16}  {:>6}  {:>10}  {:<10}  {:<10}",
            n.node_id,
            n.hostname,
            n.address,
            cores,
            mem,
            n.status,
            storage_backend_str(n.storage_backend)
        );
    }
}

pub fn print_node_detail(n: &controller_proto::NodeInfo) {
    println!("ID:        {}", n.node_id);
    println!("Hostname:  {}", n.hostname);
    println!("Address:   {}", n.address);
    println!("Status:    {}", n.status);
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
