//! `kctl` subcommands for the [`DiskLayout`] controller resource.
//!
//! The manifest surface looks like:
//!
//! ```yaml
//! kind: DiskLayout
//! metadata:
//!   name: datastore-sata
//! spec:
//!   nodeId: node-prod-01
//!   layoutNix: |
//!     { disko.devices = { disk.data = { device = "/dev/sda"; ... }; }; }
//! ```
//!
//! `layoutNixFile` is accepted as a shortcut to read the Nix body from a
//! file next to the manifest, so operators don't have to inline a
//! several-KB-long expression.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::apply_summary::render_apply_summary;
use crate::client::{self, controller_proto};
use crate::config::ConnectionInfo;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiskLayoutManifest {
    kind: String,
    metadata: ManifestMetadata,
    spec: DiskLayoutSpec,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestMetadata {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiskLayoutSpec {
    node_id: String,
    #[serde(default)]
    layout_nix: String,
    #[serde(default)]
    layout_nix_file: String,
}

pub async fn apply_from_file(info: &ConnectionInfo, file: &str) -> Result<()> {
    let manifest = parse_manifest(file)?;
    let layout_nix = resolve_layout_nix(file, &manifest.spec)?;
    if !layout_nix.contains("disko.devices") {
        bail!(
            "disk layout '{}' must define disko.devices in its layoutNix body",
            manifest.metadata.name
        );
    }

    let mut client = client::controller_client(info).await?;
    let resp = client
        .create_disk_layout(controller_proto::CreateDiskLayoutRequest {
            disk_layout: Some(controller_proto::DiskLayout {
                name: manifest.metadata.name.clone(),
                node_id: manifest.spec.node_id.trim().to_string(),
                generation: 0, // server assigns
                layout_nix,
                created_at: None,
                updated_at: None,
            }),
        })
        .await
        .context("create_disk_layout rpc")?
        .into_inner();

    let label = format!("disk layout '{}'", manifest.metadata.name);
    println!(
        "{}",
        render_apply_summary(resp.action, &resp.changed_fields, &label)
    );
    Ok(())
}

pub async fn diff_from_file(info: &ConnectionInfo, file: &str) -> Result<()> {
    let manifest = parse_manifest(file)?;
    let layout_nix = resolve_layout_nix(file, &manifest.spec)?;

    let mut client = client::controller_client(info).await?;
    let resp = client
        .classify_disk_layout(controller_proto::ClassifyDiskLayoutRequest {
            disk_layout: Some(controller_proto::DiskLayout {
                name: manifest.metadata.name.clone(),
                node_id: manifest.spec.node_id.trim().to_string(),
                generation: 0,
                layout_nix,
                created_at: None,
                updated_at: None,
            }),
        })
        .await
        .context("classify_disk_layout rpc")?
        .into_inner();

    println!("Disk layout:   {}", manifest.metadata.name);
    println!("Target node:   {}", manifest.spec.node_id);
    println!("Target disks:");
    if resp.target_devices.is_empty() {
        println!("  (none detected)");
    } else {
        for d in &resp.target_devices {
            println!("  - {d}");
        }
    }
    if resp.safe {
        println!("Pre-flight:    SAFE");
    } else {
        println!("Pre-flight:    REFUSED ({})", resp.refusal_reason);
    }
    if !resp.detail.is_empty() {
        println!("Detail:        {}", resp.detail);
    }
    println!(
        "Note: controller pre-flight is advisory. The node-agent classifier \
         runs against live lsblk state and has the final say on every apply."
    );
    Ok(())
}

pub async fn list(info: &ConnectionInfo, node_filter: Option<&str>) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .list_disk_layouts(controller_proto::ListDiskLayoutsRequest {
            node_id: node_filter.unwrap_or_default().to_string(),
        })
        .await?
        .into_inner();
    if resp.disk_layouts.is_empty() {
        println!("No disk layouts found");
        return Ok(());
    }
    println!(
        "{:<24}  {:<24}  {:>4}  {:<10}  {:<20}",
        "NAME", "NODE", "GEN", "PHASE", "REFUSAL_REASON"
    );
    for entry in resp.disk_layouts {
        let layout = entry.disk_layout.unwrap_or_default();
        let status = entry.status.unwrap_or_default();
        println!(
            "{:<24}  {:<24}  {:>4}  {:<10}  {:<20}",
            layout.name,
            layout.node_id,
            layout.generation,
            phase_str(status.phase),
            status.refusal_reason,
        );
    }
    Ok(())
}

pub async fn get(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .get_disk_layout(controller_proto::GetDiskLayoutRequest {
            name: name.to_string(),
        })
        .await?
        .into_inner();
    let layout = resp.disk_layout.context("disk layout not found")?;
    println!("Name:        {}", layout.name);
    println!("Node:        {}", layout.node_id);
    println!("Generation:  {}", layout.generation);
    if let Some(status) = resp.status {
        println!("Observed:    {}", status.observed_generation);
        println!("Phase:       {}", phase_str(status.phase));
        if !status.refusal_reason.is_empty() {
            println!("Refusal:     {}", status.refusal_reason);
        }
        if !status.message.is_empty() {
            println!("Message:     {}", status.message);
        }
    }
    println!("Layout (Nix):");
    for line in layout.layout_nix.lines() {
        println!("  {line}");
    }
    Ok(())
}

pub async fn delete(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .delete_disk_layout(controller_proto::DeleteDiskLayoutRequest {
            name: name.to_string(),
        })
        .await?
        .into_inner();
    if resp.success {
        println!("Deleted disk layout '{name}'");
    } else {
        println!("Disk layout '{name}' not found");
    }
    Ok(())
}

fn parse_manifest(file: &str) -> Result<DiskLayoutManifest> {
    let content = std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?;
    let manifest: DiskLayoutManifest = serde_yaml::from_str(&content)
        .with_context(|| format!("parsing manifest YAML in {file}"))?;
    if !manifest.kind.eq_ignore_ascii_case("DiskLayout") {
        bail!(
            "expected kind: DiskLayout in {file}, got kind: {}",
            manifest.kind
        );
    }
    if manifest.metadata.name.trim().is_empty() {
        bail!("metadata.name is required in {file}");
    }
    if manifest.spec.node_id.trim().is_empty() {
        bail!("spec.nodeId is required in {file}");
    }
    Ok(manifest)
}

fn resolve_layout_nix(manifest_path: &str, spec: &DiskLayoutSpec) -> Result<String> {
    let has_inline = !spec.layout_nix.trim().is_empty();
    let has_file = !spec.layout_nix_file.trim().is_empty();
    match (has_inline, has_file) {
        (true, true) => {
            bail!("spec.layoutNix and spec.layoutNixFile are mutually exclusive; pick one")
        }
        (true, false) => Ok(spec.layout_nix.clone()),
        (false, true) => {
            let base = Path::new(manifest_path)
                .parent()
                .unwrap_or_else(|| Path::new("."));
            let full = base.join(spec.layout_nix_file.trim());
            std::fs::read_to_string(&full)
                .with_context(|| format!("reading layoutNixFile {}", full.display()))
        }
        (false, false) => bail!("one of spec.layoutNix or spec.layoutNixFile is required"),
    }
}

fn phase_str(phase: i32) -> &'static str {
    match controller_proto::DiskLayoutPhase::try_from(phase)
        .unwrap_or(controller_proto::DiskLayoutPhase::Unspecified)
    {
        controller_proto::DiskLayoutPhase::Unspecified => "unspecified",
        controller_proto::DiskLayoutPhase::Pending => "pending",
        controller_proto::DiskLayoutPhase::Applied => "applied",
        controller_proto::DiskLayoutPhase::Refused => "refused",
        controller_proto::DiskLayoutPhase::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_with_inline_nix() {
        let manifest = r#"
kind: DiskLayout
metadata:
  name: ssd-pool
spec:
  nodeId: node-a
  layoutNix: |
    { disko.devices = { disk.data = { device = "/dev/sda"; }; }; }
"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dl.yaml");
        std::fs::write(&path, manifest).unwrap();
        let got = parse_manifest(path.to_str().unwrap()).unwrap();
        assert_eq!(got.metadata.name, "ssd-pool");
        assert_eq!(got.spec.node_id, "node-a");
        assert!(got.spec.layout_nix.contains("disko.devices"));
    }

    #[test]
    fn manifest_rejects_wrong_kind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dl.yaml");
        std::fs::write(
            &path,
            "kind: VM\nmetadata:\n  name: x\nspec:\n  nodeId: n\n",
        )
        .unwrap();
        let err = parse_manifest(path.to_str().unwrap()).unwrap_err();
        assert!(format!("{err:#}").contains("expected kind: DiskLayout"));
    }

    #[test]
    fn manifest_requires_node_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dl.yaml");
        std::fs::write(
            &path,
            "kind: DiskLayout\nmetadata:\n  name: x\nspec:\n  nodeId: \"\"\n  layoutNix: \"disko.devices = {};\"\n",
        )
        .unwrap();
        let err = parse_manifest(path.to_str().unwrap()).unwrap_err();
        assert!(format!("{err:#}").contains("spec.nodeId is required"));
    }

    #[test]
    fn resolve_requires_one_of_inline_or_file() {
        let spec = DiskLayoutSpec {
            node_id: "n".to_string(),
            layout_nix: String::new(),
            layout_nix_file: String::new(),
        };
        let err = resolve_layout_nix("/tmp/does-not-exist", &spec).unwrap_err();
        assert!(format!("{err:#}").contains("one of spec.layoutNix"));
    }

    #[test]
    fn resolve_refuses_both_inline_and_file() {
        let spec = DiskLayoutSpec {
            node_id: "n".to_string(),
            layout_nix: "disko.devices = {};".to_string(),
            layout_nix_file: "disk.nix".to_string(),
        };
        let err = resolve_layout_nix("/tmp/does-not-exist", &spec).unwrap_err();
        assert!(format!("{err:#}").contains("mutually exclusive"));
    }

    #[test]
    fn resolve_reads_file_next_to_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let nix_path = dir.path().join("disk.nix");
        std::fs::write(&nix_path, "{ disko.devices = {}; }").unwrap();
        let manifest_path = dir.path().join("dl.yaml");
        std::fs::write(&manifest_path, "ignored").unwrap();
        let spec = DiskLayoutSpec {
            node_id: "n".to_string(),
            layout_nix: String::new(),
            layout_nix_file: "disk.nix".to_string(),
        };
        let got = resolve_layout_nix(manifest_path.to_str().unwrap(), &spec).unwrap();
        assert!(got.contains("disko.devices"));
    }
}
