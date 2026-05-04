//! `kctl update` — cluster rollouts (see `docs/cluster-updates.md`).

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::client::{self, controller_proto};
use crate::config::ConnectionInfo;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClusterUpdateManifest {
    kind: String,
    metadata: Metadata,
    spec: Spec,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    target: Target,
    #[serde(default)]
    selector: Selector,
    #[serde(default)]
    strategy: Strategy,
    #[serde(default)]
    drain_vms: bool,
    #[serde(default)]
    drain_timeout_seconds: i32,
    #[serde(default)]
    activation: Activation,
    #[serde(default)]
    approval_policy: String,
    #[serde(default)]
    automatic_rollback: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    version: String,
    flake_ref: String,
    flake_rev: String,
    #[serde(default)]
    nixpkgs_rev: String,
    #[serde(default)]
    system_profile: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selector {
    #[serde(default)]
    node_ids: Vec<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    datacenters: Vec<String>,
    #[serde(default)]
    all_nodes: bool,
    #[serde(default)]
    controllers_only: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Strategy {
    #[serde(default)]
    strategy_type: String,
    #[serde(default)]
    max_unavailable: i32,
    #[serde(default)]
    batch_size: i32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Activation {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    reboot_policy: String,
}

fn parse_manifest(path: &str) -> Result<ClusterUpdateManifest> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    let m: ClusterUpdateManifest =
        serde_yaml::from_str(&raw).with_context(|| format!("parsing YAML {path}"))?;
    if m.kind != "ClusterUpdate" {
        bail!("manifest kind must be ClusterUpdate, got {}", m.kind);
    }
    Ok(m)
}

fn approval_policy_from_str(s: &str) -> i32 {
    match s.trim().to_ascii_lowercase().as_str() {
        "manual" => {
            controller_proto::ClusterUpdateApprovalPolicy::ClusterUpdateApprovalManual as i32
        }
        "auto-non-disruptive" | "auto_non_disruptive" => {
            controller_proto::ClusterUpdateApprovalPolicy::ClusterUpdateApprovalAutoNonDisruptive
                as i32
        }
        "auto" => controller_proto::ClusterUpdateApprovalPolicy::ClusterUpdateApprovalAuto as i32,
        _ => controller_proto::ClusterUpdateApprovalPolicy::ClusterUpdateApprovalUnspecified as i32,
    }
}

fn strategy_type_from_str(s: &str) -> i32 {
    match s.trim().to_ascii_lowercase().as_str() {
        "canary" => controller_proto::ClusterUpdateStrategyType::ClusterUpdateStrategyCanary as i32,
        "one-at-a-time" | "one_at_a_time" => {
            controller_proto::ClusterUpdateStrategyType::ClusterUpdateStrategyOneAtATime as i32
        }
        "batch" => controller_proto::ClusterUpdateStrategyType::ClusterUpdateStrategyBatch as i32,
        "per-dc" | "per_dc" => {
            controller_proto::ClusterUpdateStrategyType::ClusterUpdateStrategyPerDc as i32
        }
        _ => controller_proto::ClusterUpdateStrategyType::ClusterUpdateStrategyUnspecified as i32,
    }
}

fn activation_mode_from_str(s: &str) -> i32 {
    match s.trim().to_ascii_lowercase().as_str() {
        "test" => controller_proto::ClusterUpdateActivationMode::ClusterUpdateActivationTest as i32,
        "switch" => {
            controller_proto::ClusterUpdateActivationMode::ClusterUpdateActivationSwitch as i32
        }
        "boot" => controller_proto::ClusterUpdateActivationMode::ClusterUpdateActivationBoot as i32,
        "auto" => controller_proto::ClusterUpdateActivationMode::ClusterUpdateActivationAuto as i32,
        _ => {
            controller_proto::ClusterUpdateActivationMode::ClusterUpdateActivationUnspecified as i32
        }
    }
}

fn to_proto(m: ClusterUpdateManifest) -> controller_proto::ClusterUpdateSpec {
    controller_proto::ClusterUpdateSpec {
        name: m.metadata.name,
        target: Some(controller_proto::ClusterUpdateTarget {
            version: m.spec.target.version,
            flake_ref: m.spec.target.flake_ref,
            flake_rev: m.spec.target.flake_rev,
            nixpkgs_rev: m.spec.target.nixpkgs_rev,
            system_profile: m.spec.target.system_profile,
        }),
        selector: Some(controller_proto::ClusterUpdateSelector {
            node_ids: m.spec.selector.node_ids,
            labels: m.spec.selector.labels,
            datacenters: m.spec.selector.datacenters,
            all_nodes: m.spec.selector.all_nodes,
            controllers_only: m.spec.selector.controllers_only,
        }),
        strategy: Some(controller_proto::ClusterUpdateStrategy {
            r#type: strategy_type_from_str(&m.spec.strategy.strategy_type),
            max_unavailable: m.spec.strategy.max_unavailable,
            batch_size: m.spec.strategy.batch_size,
        }),
        drain_vms: m.spec.drain_vms,
        drain_timeout_seconds: m.spec.drain_timeout_seconds,
        activation_mode: activation_mode_from_str(&m.spec.activation.mode),
        reboot_policy: m.spec.activation.reboot_policy,
        approval_policy: approval_policy_from_str(&m.spec.approval_policy),
        automatic_rollback: m.spec.automatic_rollback,
    }
}

pub async fn plan(info: &ConnectionInfo, file: &str) -> Result<()> {
    let m = parse_manifest(file)?;
    let spec = to_proto(m);
    let mut client = client::controller_client(info).await?;
    let resp = client
        .plan_cluster_update(controller_proto::PlanClusterUpdateRequest { spec: Some(spec) })
        .await
        .context("plan_cluster_update rpc")?
        .into_inner();
    println!("Viable: {}", resp.viable);
    if !resp.detail.is_empty() {
        println!("Detail: {}", resp.detail);
    }
    println!("Likely requires reboot: {}", resp.likely_requires_reboot);
    println!("Target nodes:");
    for id in &resp.target_node_ids {
        println!("  - {id}");
    }
    for issue in &resp.issues {
        println!("  ! {}  {}", issue.node_id, issue.reason);
    }
    Ok(())
}

pub async fn apply(info: &ConnectionInfo, file: &str) -> Result<()> {
    let m = parse_manifest(file)?;
    let spec = to_proto(m);
    let mut client = client::controller_client(info).await?;
    let resp = client
        .create_cluster_update(controller_proto::CreateClusterUpdateRequest { spec: Some(spec) })
        .await
        .context("create_cluster_update rpc")?
        .into_inner();
    println!("action={} success={}", resp.action, resp.success);
    if let Some(u) = resp.cluster_update {
        if let Some(s) = u.spec {
            println!("cluster update {}", s.name);
        }
    }
    Ok(())
}

pub async fn get(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .get_cluster_update(controller_proto::GetClusterUpdateRequest { name: name.into() })
        .await
        .context("get_cluster_update rpc")?
        .into_inner();
    let u = resp.cluster_update.context("empty cluster_update")?;
    let spec = u.spec.context("empty spec")?;
    println!("name: {}", spec.name);
    println!("generation: {}", u.generation);
    println!("phase: {}", u.phase);
    println!("approval: {}", u.approval_status);
    for n in &resp.node_statuses {
        println!(
            "  node {}  phase={}  gen={}  err={}",
            n.node_id, n.phase, n.observed_generation, n.last_error
        );
    }
    Ok(())
}

pub async fn list_cmd(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .list_cluster_updates(controller_proto::ListClusterUpdatesRequest {})
        .await
        .context("list_cluster_updates rpc")?
        .into_inner();
    for u in resp.cluster_updates {
        let name = u.spec.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
        println!("{name}\tphase={}\tgen={}", u.phase, u.generation);
    }
    Ok(())
}

pub async fn approve(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    client
        .approve_cluster_update(controller_proto::ApproveClusterUpdateRequest { name: name.into() })
        .await
        .context("approve_cluster_update rpc")?;
    println!("approved {name}");
    Ok(())
}

pub async fn cancel(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    client
        .cancel_cluster_update(controller_proto::CancelClusterUpdateRequest { name: name.into() })
        .await
        .context("cancel_cluster_update rpc")?;
    println!("cancelled {name}");
    Ok(())
}

pub async fn rollback(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    client
        .rollback_cluster_update(controller_proto::RollbackClusterUpdateRequest {
            name: name.into(),
        })
        .await
        .context("rollback_cluster_update rpc")?;
    println!("rollback requested for {name}");
    Ok(())
}
