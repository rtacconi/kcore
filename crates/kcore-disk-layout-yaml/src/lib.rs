//! Declarative YAML for [`DiskLayout`](https://github.com/rtacconi/kcore) manifests.
//!
//! The controller and node-agent still consume **Nix** that defines `disko.devices`.
//! This crate turns a structured, reviewed YAML document into that Nix string so
//! operators never hand-author Nix for the common cases.
//!
//! # Example YAML
//!
//! ```yaml
//! spec:
//!   nodeId: node-a
//!   diskLayout:
//!     disks:
//!       - name: data1
//!         device: /dev/nvme1n1
//!         gpt:
//!           partitions:
//!             - name: kcore0
//!               size: "100%"
//!               content:
//!                 type: filesystem
//!                 format: ext4
//!                 mountpoint: /var/lib/kcore/volumes1
//! ```

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Top-level `spec.diskLayout` body (under `kind: DiskLayout`).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiskLayoutBody {
    /// One entry per whole disk (`disko.devices.disk.<name>`).
    pub disks: Vec<YamlDisk>,
    /// Optional empty `lvm_vg` stubs (e.g. when partitions use `lvm_pv`).
    #[serde(default)]
    pub lvm_volume_groups: Vec<YamlLvmVg>,
    /// Optional empty `zpool` stubs for ZFS member partitions.
    #[serde(default)]
    pub zfs_pools: Vec<YamlZfsPool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct YamlDisk {
    /// Attribute name under `disko.devices.disk` (must be a valid Nix identifier).
    pub name: String,
    /// Block device path, e.g. `/dev/nvme1n1`.
    pub device: String,
    pub gpt: YamlGpt,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct YamlGpt {
    pub partitions: Vec<YamlPartition>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct YamlPartition {
    pub name: String,
    /// e.g. `100%`, `512M`
    pub size: String,
    pub content: PartitionContent,
}

/// Partition contents supported for day-2 data disks (disko-compatible).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PartitionContent {
    Filesystem {
        format: String,
        mountpoint: String,
    },
    #[serde(rename = "lvm_pv")]
    LvmPv {
        vg: String,
    },
    Zfs {
        pool: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct YamlLvmVg {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct YamlZfsPool {
    pub name: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EmitError {
    #[error("diskLayout.disks must not be empty")]
    EmptyDisks,
    #[error("disk `{0}`: at least one GPT partition is required")]
    NoPartitions(String),
    #[error("invalid Nix identifier `{0}`: use letters, digits, underscore; must not start with a digit")]
    BadIdentifier(String),
    #[error("disk `{0}`: device must be an absolute /dev/ path")]
    BadDevicePath(String),
    #[error("duplicate disk name `{0}`")]
    DuplicateDiskName(String),
    #[error("partition `{0}` on disk `{1}`: size must not be empty")]
    EmptyPartitionSize(String, String),
}

/// Emit a Nix expression whose top-level sets `disko.devices` for disko.
pub fn emit_disko_devices_nix(body: &DiskLayoutBody) -> Result<String, EmitError> {
    if body.disks.is_empty() {
        return Err(EmitError::EmptyDisks);
    }
    let mut seen = std::collections::BTreeSet::new();
    for d in &body.disks {
        if !seen.insert(d.name.clone()) {
            return Err(EmitError::DuplicateDiskName(d.name.clone()));
        }
        validate_ident(&d.name)?;
        validate_dev_path(&d.device)?;
        if d.gpt.partitions.is_empty() {
            return Err(EmitError::NoPartitions(d.name.clone()));
        }
        for p in &d.gpt.partitions {
            validate_ident(&p.name)?;
            if p.size.trim().is_empty() {
                return Err(EmitError::EmptyPartitionSize(
                    p.name.clone(),
                    d.name.clone(),
                ));
            }
            match &p.content {
                PartitionContent::Filesystem { mountpoint, .. } => {
                    if mountpoint.is_empty() {
                        return Err(EmitError::BadIdentifier(mountpoint.clone()));
                    }
                }
                PartitionContent::LvmPv { vg } => validate_ident(vg)?,
                PartitionContent::Zfs { pool } => validate_ident(pool)?,
            }
        }
    }
    for v in &body.lvm_volume_groups {
        validate_ident(&v.name)?;
    }
    for z in &body.zfs_pools {
        validate_ident(&z.name)?;
    }

    let mut out = String::new();
    out.push_str("{ disko.devices = {\n");
    out.push_str("  disk = {\n");
    for disk in &body.disks {
        out.push_str(&format!("    {} = {{\n", disk.name));
        out.push_str("      type = \"disk\";\n");
        out.push_str(&format!("      device = {};\n", nix_string(&disk.device)));
        out.push_str("      content = {\n");
        out.push_str("        type = \"gpt\";\n");
        out.push_str("        partitions = {\n");
        for part in &disk.gpt.partitions {
            out.push_str(&format!("          {} = {{\n", part.name));
            out.push_str(&format!("            size = {};\n", nix_string(&part.size)));
            out.push_str("            content = ");
            out.push_str(&emit_partition_content(&part.content)?);
            out.push_str(";\n");
            out.push_str("          };\n");
        }
        out.push_str("        };\n");
        out.push_str("      };\n");
        out.push_str("    };\n");
    }
    out.push_str("  };\n");

    if !body.lvm_volume_groups.is_empty() {
        out.push_str("  lvm_vg = {\n");
        for vg in &body.lvm_volume_groups {
            out.push_str(&format!("    {} = {{\n", vg.name));
            out.push_str("      type = \"lvm_vg\";\n");
            out.push_str("      lvs = { };\n");
            out.push_str("    };\n");
        }
        out.push_str("  };\n");
    }

    if !body.zfs_pools.is_empty() {
        out.push_str("  zpool = {\n");
        for pool in &body.zfs_pools {
            out.push_str(&format!("    {} = {{\n", pool.name));
            out.push_str("      type = \"zpool\";\n");
            out.push_str("      datasets = { };\n");
            out.push_str("    };\n");
        }
        out.push_str("  };\n");
    }

    out.push_str("}; }");
    Ok(out)
}

fn emit_partition_content(c: &PartitionContent) -> Result<String, EmitError> {
    Ok(match c {
        PartitionContent::Filesystem { format, mountpoint } => {
            format!(
                "{{ type = \"filesystem\"; format = {}; mountpoint = {}; }}",
                nix_string(format),
                nix_string(mountpoint)
            )
        }
        PartitionContent::LvmPv { vg } => {
            format!("{{ type = \"lvm_pv\"; vg = {}; }}", nix_string(vg))
        }
        PartitionContent::Zfs { pool } => {
            format!("{{ type = \"zfs\"; pool = {}; }}", nix_string(pool))
        }
    })
}

fn validate_ident(s: &str) -> Result<(), EmitError> {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return Err(EmitError::BadIdentifier(s.to_string()));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(EmitError::BadIdentifier(s.to_string()));
    }
    if first.is_ascii_digit() {
        return Err(EmitError::BadIdentifier(s.to_string()));
    }
    for ch in chars {
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            return Err(EmitError::BadIdentifier(s.to_string()));
        }
    }
    Ok(())
}

fn validate_dev_path(s: &str) -> Result<(), EmitError> {
    if !s.starts_with("/dev/") || s.len() < 6 {
        return Err(EmitError::BadDevicePath(s.to_string()));
    }
    Ok(())
}

/// Escape a string for use inside Nix double quotes.
fn nix_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_ascii() => out.push(c),
            c => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_ext4() -> DiskLayoutBody {
        DiskLayoutBody {
            disks: vec![YamlDisk {
                name: "data1".into(),
                device: "/dev/nvme1n1".into(),
                gpt: YamlGpt {
                    partitions: vec![YamlPartition {
                        name: "kcore0".into(),
                        size: "100%".into(),
                        content: PartitionContent::Filesystem {
                            format: "ext4".into(),
                            mountpoint: "/var/lib/kcore/volumes1".into(),
                        },
                    }],
                },
            }],
            lvm_volume_groups: vec![],
            zfs_pools: vec![],
        }
    }

    #[test]
    fn emit_contains_disko_devices_and_device() {
        let nix = emit_disko_devices_nix(&minimal_ext4()).unwrap();
        assert!(nix.contains("disko.devices"));
        assert!(nix.contains("device = \"/dev/nvme1n1\""));
        assert!(nix.contains("type = \"filesystem\""));
        assert!(nix.contains("/var/lib/kcore/volumes1"));
    }

    #[test]
    fn extract_target_devices_compatible() {
        let nix = emit_disko_devices_nix(&minimal_ext4()).unwrap();
        // kcore-disko-types extractor keys off `device = "/dev/...`
        assert!(nix.contains("device = \"/dev/nvme1n1\""));
    }

    #[test]
    fn rejects_invalid_disk_name() {
        let mut b = minimal_ext4();
        b.disks[0].name = "123bad".into();
        assert!(emit_disko_devices_nix(&b).is_err());
    }

    #[test]
    fn lvm_vg_and_zpool_emit() {
        let body = DiskLayoutBody {
            disks: vec![YamlDisk {
                name: "data1".into(),
                device: "/dev/sdb".into(),
                gpt: YamlGpt {
                    partitions: vec![YamlPartition {
                        name: "pv0".into(),
                        size: "100%".into(),
                        content: PartitionContent::LvmPv {
                            vg: "vg_kcore".into(),
                        },
                    }],
                },
            }],
            lvm_volume_groups: vec![YamlLvmVg {
                name: "vg_kcore".into(),
            }],
            zfs_pools: vec![],
        };
        let nix = emit_disko_devices_nix(&body).unwrap();
        assert!(nix.contains("lvm_vg"));
        assert!(nix.contains("lvm_pv"));
    }
}
