use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::client::{self, controller_proto};
use crate::config::ConnectionInfo;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecurityGroupManifest {
    kind: String,
    metadata: ManifestMetadata,
    #[serde(default)]
    spec: SecurityGroupSpec,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestMetadata {
    name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecurityGroupSpec {
    #[serde(default)]
    description: String,
    #[serde(default)]
    rules: Vec<SecurityGroupRuleManifest>,
    #[serde(default)]
    attachments: Vec<SecurityGroupAttachmentManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecurityGroupRuleManifest {
    #[serde(default)]
    id: String,
    protocol: String,
    host_port: i32,
    #[serde(default)]
    target_port: i32,
    #[serde(default)]
    source_cidr: String,
    #[serde(default)]
    target_vm: String,
    #[serde(default)]
    enable_dnat: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecurityGroupAttachmentManifest {
    kind: String,
    target: String,
    #[serde(default)]
    node: String,
}

pub async fn create_from_file(info: &ConnectionInfo, file: &str) -> Result<()> {
    let manifest = parse_manifest(file)?;
    create_from_manifest(info, &manifest).await?;
    apply_manifest_attachments(info, &manifest, false).await?;
    Ok(())
}

pub async fn apply_from_file(info: &ConnectionInfo, file: &str) -> Result<()> {
    let manifest = parse_manifest(file)?;
    create_from_manifest(info, &manifest).await?;
    apply_manifest_attachments(info, &manifest, true).await?;
    Ok(())
}

pub async fn list(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .list_security_groups(controller_proto::ListSecurityGroupsRequest {})
        .await?
        .into_inner();
    if resp.security_groups.is_empty() {
        println!("No security groups found");
        return Ok(());
    }
    println!("{:<24}  {:>5}  {:<40}", "NAME", "RULES", "DESCRIPTION");
    for sg in resp.security_groups {
        println!(
            "{:<24}  {:>5}  {:<40}",
            sg.name,
            sg.rules.len(),
            sg.description
        );
    }
    Ok(())
}

pub async fn get(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .get_security_group(controller_proto::GetSecurityGroupRequest {
            name: name.to_string(),
        })
        .await?
        .into_inner();
    let sg = resp.security_group.context("security group not found")?;
    println!("Name:        {}", sg.name);
    println!("Description: {}", sg.description);
    println!("Rules:");
    if sg.rules.is_empty() {
        println!("  (none)");
    } else {
        for r in sg.rules {
            println!(
                "  - {} {}:{} -> {}{}{}",
                r.protocol,
                if r.source_cidr.is_empty() {
                    "0.0.0.0/0"
                } else {
                    &r.source_cidr
                },
                r.host_port,
                r.target_port,
                if r.enable_dnat { " dnat" } else { "" },
                if r.target_vm.is_empty() {
                    String::new()
                } else {
                    format!(" target_vm={}", r.target_vm)
                }
            );
        }
    }
    println!("Attachments:");
    if resp.attachments.is_empty() {
        println!("  (none)");
    } else {
        for a in resp.attachments {
            let kind = controller_proto::SecurityGroupTargetKind::try_from(a.target_kind)
                .unwrap_or(controller_proto::SecurityGroupTargetKind::Unspecified);
            match kind {
                controller_proto::SecurityGroupTargetKind::Vm => {
                    println!("  - vm: {}", a.target_id);
                }
                controller_proto::SecurityGroupTargetKind::Network => {
                    println!("  - network: {} (node: {})", a.target_id, a.target_node);
                }
                controller_proto::SecurityGroupTargetKind::Unspecified => {
                    println!("  - unknown: {}", a.target_id);
                }
            }
        }
    }
    Ok(())
}

pub async fn delete(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    client
        .delete_security_group(controller_proto::DeleteSecurityGroupRequest {
            name: name.to_string(),
        })
        .await?;
    println!("Deleted security group '{name}'");
    Ok(())
}

pub async fn attach(
    info: &ConnectionInfo,
    name: &str,
    kind: &str,
    target: &str,
    target_node: Option<&str>,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    client
        .attach_security_group(controller_proto::AttachSecurityGroupRequest {
            security_group: name.to_string(),
            target_kind: target_kind(kind)? as i32,
            target_id: target.to_string(),
            target_node: target_node.unwrap_or_default().to_string(),
        })
        .await?;
    println!("Attached security group '{name}' to {kind} '{target}'");
    Ok(())
}

pub async fn detach(
    info: &ConnectionInfo,
    name: &str,
    kind: &str,
    target: &str,
    target_node: Option<&str>,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    client
        .detach_security_group(controller_proto::DetachSecurityGroupRequest {
            security_group: name.to_string(),
            target_kind: target_kind(kind)? as i32,
            target_id: target.to_string(),
            target_node: target_node.unwrap_or_default().to_string(),
        })
        .await?;
    println!("Detached security group '{name}' from {kind} '{target}'");
    Ok(())
}

fn parse_manifest(path: &str) -> Result<SecurityGroupManifest> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading security group manifest: {path}"))?;
    let manifest: SecurityGroupManifest = serde_yaml::from_str(&content)
        .with_context(|| format!("parsing security group manifest YAML: {path}"))?;
    if manifest.kind != "SecurityGroup" {
        bail!(
            "manifest kind must be SecurityGroup, found '{}'",
            manifest.kind
        );
    }
    if manifest.metadata.name.trim().is_empty() {
        bail!("metadata.name is required");
    }
    Ok(manifest)
}

async fn create_from_manifest(info: &ConnectionInfo, manifest: &SecurityGroupManifest) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let rules = manifest
        .spec
        .rules
        .iter()
        .map(|r| controller_proto::SecurityGroupRule {
            id: r.id.clone(),
            protocol: r.protocol.clone(),
            host_port: r.host_port,
            target_port: r.target_port,
            source_cidr: r.source_cidr.clone(),
            target_vm: r.target_vm.clone(),
            enable_dnat: r.enable_dnat,
        })
        .collect();
    client
        .create_security_group(controller_proto::CreateSecurityGroupRequest {
            security_group: Some(controller_proto::SecurityGroup {
                name: manifest.metadata.name.clone(),
                description: manifest.spec.description.clone(),
                rules,
                created_at: None,
            }),
        })
        .await?;
    println!("Applied security group '{}'", manifest.metadata.name);
    Ok(())
}

async fn apply_manifest_attachments(
    info: &ConnectionInfo,
    manifest: &SecurityGroupManifest,
    replace: bool,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let current = client
        .get_security_group(controller_proto::GetSecurityGroupRequest {
            name: manifest.metadata.name.clone(),
        })
        .await?
        .into_inner()
        .attachments;

    let desired: Vec<(controller_proto::SecurityGroupTargetKind, String, String)> = manifest
        .spec
        .attachments
        .iter()
        .map(|a| {
            Ok((
                target_kind(&a.kind)?,
                a.target.trim().to_string(),
                a.node.trim().to_string(),
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    for (kind, target, node) in &desired {
        client
            .attach_security_group(controller_proto::AttachSecurityGroupRequest {
                security_group: manifest.metadata.name.clone(),
                target_kind: *kind as i32,
                target_id: target.clone(),
                target_node: node.clone(),
            })
            .await?;
    }

    if replace {
        for att in current {
            let kind = controller_proto::SecurityGroupTargetKind::try_from(att.target_kind)
                .unwrap_or(controller_proto::SecurityGroupTargetKind::Unspecified);
            let exists = desired
                .iter()
                .any(|(k, t, n)| *k == kind && *t == att.target_id && *n == att.target_node);
            if !exists {
                client
                    .detach_security_group(controller_proto::DetachSecurityGroupRequest {
                        security_group: manifest.metadata.name.clone(),
                        target_kind: kind as i32,
                        target_id: att.target_id,
                        target_node: att.target_node,
                    })
                    .await?;
            }
        }
    }

    Ok(())
}

fn target_kind(kind: &str) -> Result<controller_proto::SecurityGroupTargetKind> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "vm" => Ok(controller_proto::SecurityGroupTargetKind::Vm),
        "network" => Ok(controller_proto::SecurityGroupTargetKind::Network),
        _ => bail!("attachment kind must be vm or network"),
    }
}
