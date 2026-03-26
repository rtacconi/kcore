use crate::db::NodeRow;

/// Pick the ready node with the most free memory after accounting for
/// allocated VMs. Falls back to first-ready when no allocation data
/// is available.
pub fn select_node(nodes: &[NodeRow]) -> Option<&NodeRow> {
    nodes
        .iter()
        .filter(|n| n.status == "ready")
        .max_by_key(|n| {
            let free_mem = n.memory_bytes - n.memory_used;
            let free_cpu = (n.cpu_cores - n.cpu_used) as i64;
            (free_mem, free_cpu)
        })
}

/// Pick the ready node that can fit the requested resources and has the
/// most remaining capacity afterwards.
pub fn select_node_for_vm(
    nodes: &[NodeRow],
    requested_cpu: i32,
    requested_memory: i64,
) -> Option<&NodeRow> {
    nodes
        .iter()
        .filter(|n| {
            n.status == "ready"
                && (n.cpu_cores - n.cpu_used) >= requested_cpu
                && (n.memory_bytes - n.memory_used) >= requested_memory
        })
        .max_by_key(|n| {
            let free_mem = n.memory_bytes - n.memory_used;
            let free_cpu = (n.cpu_cores - n.cpu_used) as i64;
            (free_mem, free_cpu)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, cpu: i32, mem: i64, cpu_used: i32, mem_used: i64) -> NodeRow {
        NodeRow {
            id: id.into(),
            hostname: id.into(),
            address: format!("{id}:9091"),
            cpu_cores: cpu,
            memory_bytes: mem,
            status: "ready".into(),
            last_heartbeat: String::new(),
            gateway_interface: String::new(),
            cpu_used,
            memory_used: mem_used,
            storage_backend: "filesystem".into(),
            disable_vxlan: false,
        }
    }

    #[test]
    fn selects_node_with_most_free_memory() {
        let nodes = vec![
            node("n1", 8, 16_000_000_000, 4, 12_000_000_000),
            node("n2", 8, 16_000_000_000, 2, 4_000_000_000),
        ];
        let picked = select_node(&nodes).unwrap();
        assert_eq!(picked.id, "n2");
    }

    #[test]
    fn select_node_for_vm_respects_capacity() {
        let nodes = vec![
            node("n1", 4, 8_000_000_000, 3, 6_000_000_000),
            node("n2", 8, 16_000_000_000, 2, 4_000_000_000),
        ];
        let picked = select_node_for_vm(&nodes, 4, 8_000_000_000).unwrap();
        assert_eq!(picked.id, "n2");
    }

    #[test]
    fn select_node_for_vm_returns_none_when_no_fit() {
        let nodes = vec![node("n1", 4, 8_000_000_000, 4, 8_000_000_000)];
        assert!(select_node_for_vm(&nodes, 2, 4_000_000_000).is_none());
    }

    #[test]
    fn skips_non_ready_nodes() {
        let mut n = node("n1", 8, 16_000_000_000, 0, 0);
        n.status = "unknown".into();
        assert!(select_node(&[n]).is_none());
    }
}
