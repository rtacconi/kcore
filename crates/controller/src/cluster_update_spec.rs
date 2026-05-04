//! Serialize/deserialize [`ClusterUpdateSpec`](crate::controller_proto::ClusterUpdateSpec)
//! for SQLite and resolve node selectors.

use crate::controller_proto::{
    ClusterUpdateActivationMode, ClusterUpdateApprovalPolicy, ClusterUpdateSelector,
    ClusterUpdateSpec, ClusterUpdateStrategy, ClusterUpdateTarget,
};
use crate::db::{Database, NodeRow};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredClusterUpdateSpec {
    pub name: String,
    pub target: StoredTarget,
    pub selector: StoredSelector,
    pub strategy: StoredStrategy,
    #[serde(default)]
    pub drain_vms: bool,
    #[serde(default)]
    pub drain_timeout_seconds: i32,
    pub activation_mode: i32,
    #[serde(default)]
    pub reboot_policy: String,
    pub approval_policy: i32,
    #[serde(default)]
    pub automatic_rollback: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredTarget {
    pub version: String,
    pub flake_ref: String,
    pub flake_rev: String,
    #[serde(default)]
    pub nixpkgs_rev: String,
    #[serde(default)]
    pub system_profile: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredSelector {
    #[serde(default)]
    pub node_ids: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub datacenters: Vec<String>,
    #[serde(default)]
    pub all_nodes: bool,
    #[serde(default)]
    pub controllers_only: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredStrategy {
    pub strategy_type: i32,
    #[serde(default)]
    pub max_unavailable: i32,
    #[serde(default)]
    pub batch_size: i32,
}

pub fn spec_to_json(spec: &ClusterUpdateSpec) -> Result<String, serde_json::Error> {
    let stored = stored_from_proto(spec);
    serde_json::to_string(&stored)
}

pub fn spec_from_json(s: &str) -> Result<ClusterUpdateSpec, serde_json::Error> {
    let stored: StoredClusterUpdateSpec = serde_json::from_str(s)?;
    Ok(proto_from_stored(stored))
}

fn stored_from_proto(spec: &ClusterUpdateSpec) -> StoredClusterUpdateSpec {
    let target = spec.target.clone().unwrap_or_default();
    let selector = spec.selector.clone().unwrap_or_default();
    let strategy = spec.strategy.unwrap_or_default();
    StoredClusterUpdateSpec {
        name: spec.name.clone(),
        target: StoredTarget {
            version: target.version,
            flake_ref: target.flake_ref,
            flake_rev: target.flake_rev,
            nixpkgs_rev: target.nixpkgs_rev,
            system_profile: target.system_profile,
        },
        selector: StoredSelector {
            node_ids: selector.node_ids.clone(),
            labels: selector.labels.clone(),
            datacenters: selector.datacenters.clone(),
            all_nodes: selector.all_nodes,
            controllers_only: selector.controllers_only,
        },
        strategy: StoredStrategy {
            strategy_type: strategy.r#type,
            max_unavailable: strategy.max_unavailable,
            batch_size: strategy.batch_size,
        },
        drain_vms: spec.drain_vms,
        drain_timeout_seconds: spec.drain_timeout_seconds,
        activation_mode: spec.activation_mode,
        reboot_policy: spec.reboot_policy.clone(),
        approval_policy: spec.approval_policy,
        automatic_rollback: spec.automatic_rollback,
    }
}

fn proto_from_stored(s: StoredClusterUpdateSpec) -> ClusterUpdateSpec {
    ClusterUpdateSpec {
        name: s.name,
        target: Some(ClusterUpdateTarget {
            version: s.target.version,
            flake_ref: s.target.flake_ref,
            flake_rev: s.target.flake_rev,
            nixpkgs_rev: s.target.nixpkgs_rev,
            system_profile: s.target.system_profile,
        }),
        selector: Some(ClusterUpdateSelector {
            node_ids: s.selector.node_ids,
            labels: s.selector.labels,
            datacenters: s.selector.datacenters,
            all_nodes: s.selector.all_nodes,
            controllers_only: s.selector.controllers_only,
        }),
        strategy: Some(ClusterUpdateStrategy {
            r#type: s.strategy.strategy_type,
            max_unavailable: s.strategy.max_unavailable,
            batch_size: s.strategy.batch_size,
        }),
        drain_vms: s.drain_vms,
        drain_timeout_seconds: s.drain_timeout_seconds,
        activation_mode: s.activation_mode,
        reboot_policy: s.reboot_policy,
        approval_policy: s.approval_policy,
        automatic_rollback: s.automatic_rollback,
    }
}

pub fn resolve_target_node_ids(
    db: &Database,
    spec: &ClusterUpdateSpec,
) -> Result<Vec<String>, String> {
    let selector = spec
        .selector
        .as_ref()
        .ok_or_else(|| "cluster update selector is required".to_string())?;
    let all_nodes = db.list_nodes().map_err(|e| format!("list nodes: {e}"))?;
    let mut out: Vec<String> = Vec::new();
    'next: for n in all_nodes {
        if !node_matches_selector(db, &n, selector)? {
            continue 'next;
        }
        out.push(n.id.clone());
    }
    out.sort();
    Ok(out)
}

fn node_matches_selector(
    db: &Database,
    node: &NodeRow,
    selector: &ClusterUpdateSelector,
) -> Result<bool, String> {
    if selector.all_nodes {
        return Ok(true);
    }
    if selector.controllers_only && !node.hostname.to_ascii_lowercase().contains("controller") {
        return Ok(false);
    }
    if !selector.node_ids.is_empty() && !selector.node_ids.iter().any(|id| id == &node.id) {
        return Ok(false);
    }
    if !selector.datacenters.is_empty() {
        let dc = node.dc_id.trim();
        if dc.is_empty() || !selector.datacenters.iter().any(|d| d == dc) {
            return Ok(false);
        }
    }
    if !selector.labels.is_empty() {
        let labels = db
            .get_node_labels(&node.id)
            .map_err(|e| format!("labels for {}: {e}", node.id))?;
        for want in &selector.labels {
            if !labels.iter().any(|l| l == want) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub fn validate_spec(spec: &ClusterUpdateSpec) -> Result<(), String> {
    if spec.name.trim().is_empty() {
        return Err("spec.name is required".into());
    }
    let t = spec
        .target
        .as_ref()
        .ok_or_else(|| "spec.target is required".to_string())?;
    if t.flake_ref.trim().is_empty() {
        return Err("spec.target.flake_ref is required".into());
    }
    if t.flake_rev.trim().is_empty() {
        return Err("spec.target.flake_rev is required before rollout".into());
    }
    let sel = spec
        .selector
        .as_ref()
        .ok_or_else(|| "selector required".to_string())?;
    if !sel.all_nodes
        && sel.node_ids.is_empty()
        && sel.datacenters.is_empty()
        && sel.labels.is_empty()
        && !sel.controllers_only
    {
        return Err(
            "selector must set all_nodes, node_ids, datacenters, labels, or controllers_only"
                .into(),
        );
    }
    Ok(())
}

pub fn activation_mode_string(spec: &ClusterUpdateSpec) -> &'static str {
    match spec.activation_mode {
        x if x == ClusterUpdateActivationMode::ClusterUpdateActivationTest as i32 => "test",
        x if x == ClusterUpdateActivationMode::ClusterUpdateActivationSwitch as i32 => "switch",
        x if x == ClusterUpdateActivationMode::ClusterUpdateActivationBoot as i32 => "boot",
        x if x == ClusterUpdateActivationMode::ClusterUpdateActivationAuto as i32 => "switch",
        _ => "switch",
    }
}

pub fn requires_manual_approval(spec: &ClusterUpdateSpec) -> bool {
    spec.approval_policy == ClusterUpdateApprovalPolicy::ClusterUpdateApprovalManual as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller_proto::ClusterUpdateStrategyType;
    use crate::db::{Database, NodeRow};

    fn one_at_a_time_spec() -> ClusterUpdateSpec {
        ClusterUpdateSpec {
            name: "u1".into(),
            target: Some(ClusterUpdateTarget {
                version: "0.3".into(),
                flake_ref: "github:x/y".into(),
                flake_rev: "abc".into(),
                nixpkgs_rev: "".into(),
                system_profile: "".into(),
            }),
            selector: Some(ClusterUpdateSelector {
                all_nodes: true,
                ..Default::default()
            }),
            strategy: Some(ClusterUpdateStrategy {
                r#type: ClusterUpdateStrategyType::ClusterUpdateStrategyOneAtATime as i32,
                max_unavailable: 1,
                batch_size: 0,
            }),
            drain_vms: false,
            drain_timeout_seconds: 0,
            activation_mode: ClusterUpdateActivationMode::ClusterUpdateActivationSwitch as i32,
            reboot_policy: "if-required".into(),
            approval_policy: ClusterUpdateApprovalPolicy::ClusterUpdateApprovalManual as i32,
            automatic_rollback: false,
        }
    }

    #[test]
    fn round_trip_spec_json() {
        let spec = one_at_a_time_spec();
        let j = spec_to_json(&spec).unwrap();
        let back = spec_from_json(&j).unwrap();
        assert_eq!(back.name, spec.name);
        assert_eq!(
            back.target.as_ref().unwrap().flake_rev,
            spec.target.as_ref().unwrap().flake_rev
        );
        assert_eq!(back.activation_mode, spec.activation_mode);
        assert_eq!(back.approval_policy, spec.approval_policy);
        assert_eq!(
            back.strategy.as_ref().unwrap().r#type,
            spec.strategy.as_ref().unwrap().r#type
        );
        assert!(back.selector.as_ref().unwrap().all_nodes);
    }

    #[test]
    fn validate_rejects_missing_target_or_selector() {
        let mut spec = one_at_a_time_spec();
        spec.target = None;
        assert!(validate_spec(&spec).is_err());

        let mut spec = one_at_a_time_spec();
        spec.selector = Some(ClusterUpdateSelector::default());
        assert!(validate_spec(&spec).is_err(), "empty selector must fail");
    }

    #[test]
    fn validate_requires_flake_rev() {
        let mut spec = one_at_a_time_spec();
        spec.target.as_mut().unwrap().flake_rev.clear();
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn activation_mode_string_defaults_to_switch() {
        let mut spec = one_at_a_time_spec();
        spec.activation_mode =
            ClusterUpdateActivationMode::ClusterUpdateActivationUnspecified as i32;
        assert_eq!(activation_mode_string(&spec), "switch");
        spec.activation_mode = ClusterUpdateActivationMode::ClusterUpdateActivationTest as i32;
        assert_eq!(activation_mode_string(&spec), "test");
        spec.activation_mode = ClusterUpdateActivationMode::ClusterUpdateActivationBoot as i32;
        assert_eq!(activation_mode_string(&spec), "boot");
        spec.activation_mode = ClusterUpdateActivationMode::ClusterUpdateActivationAuto as i32;
        assert_eq!(activation_mode_string(&spec), "switch");
    }

    fn make_node(id: &str, dc: &str) -> NodeRow {
        NodeRow {
            id: id.into(),
            hostname: id.into(),
            address: "127.0.0.1:9091".into(),
            cpu_cores: 0,
            memory_bytes: 0,
            status: "online".into(),
            last_heartbeat: String::new(),
            gateway_interface: String::new(),
            cpu_used: 0,
            memory_used: 0,
            storage_backend: "filesystem".into(),
            disable_vxlan: false,
            approval_status: "approved".into(),
            cert_expiry_days: 0,
            luks_method: String::new(),
            dc_id: dc.into(),
        }
    }

    #[test]
    fn resolve_target_node_ids_filters_by_dc() {
        let db = Database::open(":memory:").unwrap();
        db.upsert_node(&make_node("a", "DC1")).unwrap();
        db.upsert_node(&make_node("b", "DC2")).unwrap();
        db.upsert_node(&make_node("c", "DC1")).unwrap();

        let mut spec = one_at_a_time_spec();
        spec.selector = Some(ClusterUpdateSelector {
            datacenters: vec!["DC1".into()],
            ..Default::default()
        });
        let mut ids = resolve_target_node_ids(&db, &spec).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn resolve_target_node_ids_filters_by_label() {
        let db = Database::open(":memory:").unwrap();
        db.upsert_node(&make_node("a", "DC1")).unwrap();
        db.upsert_node(&make_node("b", "DC1")).unwrap();
        db.upsert_node_labels("a", &["role=worker".into()]).unwrap();

        let mut spec = one_at_a_time_spec();
        spec.selector = Some(ClusterUpdateSelector {
            labels: vec!["role=worker".into()],
            ..Default::default()
        });
        let ids = resolve_target_node_ids(&db, &spec).unwrap();
        assert_eq!(ids, vec!["a".to_string()]);
    }

    #[test]
    fn resolve_target_node_ids_all_nodes_includes_every_node() {
        let db = Database::open(":memory:").unwrap();
        db.upsert_node(&make_node("a", "DC1")).unwrap();
        db.upsert_node(&make_node("b", "DC2")).unwrap();
        let mut spec = one_at_a_time_spec();
        spec.selector = Some(ClusterUpdateSelector {
            all_nodes: true,
            ..Default::default()
        });
        let mut ids = resolve_target_node_ids(&db, &spec).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }
}
