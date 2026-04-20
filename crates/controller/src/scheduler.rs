use crate::db::NodeRow;

/// Pick the ready node with the most free memory after accounting for
/// allocated VMs. Falls back to first-ready when no allocation data
/// is available.
pub fn select_node(nodes: &[NodeRow]) -> Option<&NodeRow> {
    nodes
        .iter()
        .filter(|n| n.status == "ready" && n.approval_status == "approved")
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
                && n.approval_status == "approved"
                && (n.cpu_cores - n.cpu_used) >= requested_cpu
                && (n.memory_bytes - n.memory_used) >= requested_memory
        })
        .max_by_key(|n| {
            let free_mem = n.memory_bytes - n.memory_used;
            let free_cpu = (n.cpu_cores - n.cpu_used) as i64;
            (free_mem, free_cpu)
        })
}

/// Like `select_node_for_vm` but restricts candidates to nodes in the
/// specified datacenter. Returns `None` if no node in that DC has
/// sufficient capacity.
pub fn select_node_for_vm_in_dc<'a>(
    nodes: &'a [NodeRow],
    requested_cpu: i32,
    requested_memory: i64,
    dc_id: &str,
) -> Option<&'a NodeRow> {
    let dc_nodes: Vec<&NodeRow> = nodes.iter().filter(|n| n.dc_id == dc_id).collect();
    dc_nodes
        .into_iter()
        .filter(|n| {
            n.status == "ready"
                && n.approval_status == "approved"
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
            approval_status: "approved".into(),
            cert_expiry_days: -1,
            luks_method: String::new(),
            dc_id: "DC1".into(),
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

    #[test]
    fn select_node_for_vm_in_dc_filters_by_dc() {
        let mut n1 = node("n1", 8, 16_000_000_000, 0, 0);
        n1.dc_id = "DC1".into();
        let mut n2 = node("n2", 8, 16_000_000_000, 0, 0);
        n2.dc_id = "DC2".into();
        let nodes = vec![n1, n2];

        let picked = select_node_for_vm_in_dc(&nodes, 2, 4_000_000_000, "DC2").unwrap();
        assert_eq!(picked.id, "n2");
    }

    #[test]
    fn select_node_for_vm_in_dc_returns_none_for_empty_dc() {
        let nodes = vec![node("n1", 8, 16_000_000_000, 0, 0)];
        assert!(select_node_for_vm_in_dc(&nodes, 2, 4_000_000_000, "DC2").is_none());
    }

    #[test]
    fn select_node_for_vm_in_dc_picks_best_in_dc() {
        let mut n1 = node("n1", 8, 16_000_000_000, 6, 12_000_000_000);
        n1.dc_id = "DC1".into();
        let mut n2 = node("n2", 8, 16_000_000_000, 2, 4_000_000_000);
        n2.dc_id = "DC1".into();
        let mut n3 = node("n3", 8, 32_000_000_000, 0, 0);
        n3.dc_id = "DC2".into();
        let nodes = vec![n1, n2, n3];

        let picked = select_node_for_vm_in_dc(&nodes, 2, 4_000_000_000, "DC1").unwrap();
        assert_eq!(picked.id, "n2");
    }
}

/// Property-based tests (Phase 2) — node placement.
///
/// `select_node_for_vm` is invoked on every VM create/migrate; the
/// strongest guarantees we want are: (a) it never picks an ineligible
/// node (wrong status / approval / DC, insufficient capacity), and (b)
/// the picked node is **maximal** w.r.t. the documented `(free_mem,
/// free_cpu)` ordering among the eligible set.
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_node() -> impl Strategy<Value = NodeRow> {
        (
            "[a-z][a-z0-9-]{0,7}", // id
            1i32..=64,             // cpu_cores
            (1i64..=(1i64 << 38)), // memory_bytes
            0i32..=64,             // cpu_used (clamped below)
            0i64..=(1i64 << 38),   // memory_used (clamped below)
            prop::sample::select(vec!["ready", "unknown", "draining"]),
            prop::sample::select(vec!["approved", "pending", "rejected"]),
            prop::sample::select(vec!["DC1", "DC2", "DC3"]),
        )
            .prop_map(
                |(id, cpu, mem, cpu_used, mem_used, status, approval, dc)| NodeRow {
                    id: id.clone(),
                    hostname: id.clone(),
                    address: format!("{id}:9091"),
                    cpu_cores: cpu,
                    memory_bytes: mem,
                    status: status.into(),
                    last_heartbeat: String::new(),
                    gateway_interface: String::new(),
                    cpu_used: cpu_used.min(cpu),
                    memory_used: mem_used.min(mem),
                    storage_backend: "filesystem".into(),
                    disable_vxlan: false,
                    approval_status: approval.into(),
                    cert_expiry_days: -1,
                    luks_method: String::new(),
                    dc_id: dc.into(),
                },
            )
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 1_000,
            .. ProptestConfig::default()
        })]

        /// `select_node` only ever returns `ready` + `approved` nodes.
        #[test]
        fn select_node_only_returns_eligible_nodes(
            nodes in proptest::collection::vec(arb_node(), 0..10),
        ) {
            if let Some(picked) = select_node(&nodes) {
                prop_assert_eq!(picked.status.as_str(), "ready");
                prop_assert_eq!(picked.approval_status.as_str(), "approved");
            }
        }

        /// `select_node_for_vm` never returns a node whose remaining
        /// capacity is less than what was requested.
        #[test]
        fn select_node_for_vm_respects_capacity(
            nodes in proptest::collection::vec(arb_node(), 0..10),
            requested_cpu in 1i32..=32,
            requested_mem in 1i64..=(1i64 << 36),
        ) {
            if let Some(picked) = select_node_for_vm(&nodes, requested_cpu, requested_mem) {
                prop_assert!(picked.status == "ready");
                prop_assert!(picked.approval_status == "approved");
                prop_assert!((picked.cpu_cores - picked.cpu_used) >= requested_cpu);
                prop_assert!((picked.memory_bytes - picked.memory_used) >= requested_mem);
            }
        }

        /// **Maximality**: the picked node has the largest `(free_mem,
        /// free_cpu)` tuple among eligible nodes. No eligible node may
        /// have a strictly larger tuple than the winner.
        #[test]
        fn select_node_for_vm_picks_max_remaining_capacity(
            nodes in proptest::collection::vec(arb_node(), 0..10),
            requested_cpu in 1i32..=8,
            requested_mem in 1i64..=(1i64 << 32),
        ) {
            let picked = select_node_for_vm(&nodes, requested_cpu, requested_mem);
            let eligible: Vec<&NodeRow> = nodes
                .iter()
                .filter(|n| {
                    n.status == "ready"
                        && n.approval_status == "approved"
                        && (n.cpu_cores - n.cpu_used) >= requested_cpu
                        && (n.memory_bytes - n.memory_used) >= requested_mem
                })
                .collect();
            prop_assert_eq!(picked.is_some(), !eligible.is_empty());
            if let Some(p) = picked {
                let p_key = (p.memory_bytes - p.memory_used, (p.cpu_cores - p.cpu_used) as i64);
                for n in &eligible {
                    let k = (n.memory_bytes - n.memory_used, (n.cpu_cores - n.cpu_used) as i64);
                    prop_assert!(k <= p_key, "eligible node {} has larger key {:?} than winner {:?}", n.id, k, p_key);
                }
            }
        }

        /// `select_node_for_vm_in_dc` is `select_node_for_vm` after
        /// filtering by `dc_id`. We assert the filter equivalence.
        #[test]
        fn select_node_for_vm_in_dc_equivalent_to_dc_prefilter(
            nodes in proptest::collection::vec(arb_node(), 0..10),
            requested_cpu in 1i32..=8,
            requested_mem in 1i64..=(1i64 << 32),
            dc in prop::sample::select(vec!["DC1", "DC2", "DC3"]),
        ) {
            let dc_only: Vec<NodeRow> =
                nodes.iter().filter(|n| n.dc_id == dc).cloned().collect();
            let from_helper =
                select_node_for_vm_in_dc(&nodes, requested_cpu, requested_mem, dc).map(|n| n.id.clone());
            let from_prefilter = select_node_for_vm(&dc_only, requested_cpu, requested_mem)
                .map(|n| n.id.clone());
            prop_assert_eq!(from_helper, from_prefilter);
        }
    }
}
