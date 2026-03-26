use anyhow::Context;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub node_id: String,
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default)]
    pub controller_addr: String,
    pub tls: Option<TlsConfig>,
    #[serde(default = "default_vm_socket_dir")]
    pub vm_socket_dir: String,
    #[serde(default = "default_nix_config_path")]
    pub nix_config_path: String,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsConfig {
    pub ca_file: String,
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageConfig {
    #[serde(default)]
    pub backend: StorageBackendKind,
    #[serde(default = "default_image_cache_dir")]
    pub image_cache_dir: String,
    #[serde(default = "default_filesystem_volume_dir")]
    pub filesystem_volume_dir: String,
    #[serde(default)]
    pub lvm: Option<LvmConfig>,
    #[serde(default)]
    pub zfs: Option<ZfsConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StorageBackendKind {
    #[default]
    Filesystem,
    Lvm,
    Zfs,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LvmConfig {
    pub vg_name: String,
    #[serde(default = "default_lvm_lv_prefix")]
    pub lv_prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZfsConfig {
    pub pool_name: String,
    #[serde(default = "default_zfs_dataset_prefix")]
    pub dataset_prefix: String,
}

fn default_listen_addr() -> String {
    "0.0.0.0:9091".to_string()
}

fn default_vm_socket_dir() -> String {
    "/run/kcore".to_string()
}

fn default_nix_config_path() -> String {
    "/etc/nixos/kcore-vms.nix".to_string()
}

fn default_image_cache_dir() -> String {
    "/var/lib/kcore/images".to_string()
}

fn default_filesystem_volume_dir() -> String {
    "/var/lib/kcore/volumes".to_string()
}

fn default_lvm_lv_prefix() -> String {
    "kcore-".to_string()
}

fn default_zfs_dataset_prefix() -> String {
    "kcore-".to_string()
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackendKind::Filesystem,
            image_cache_dir: default_image_cache_dir(),
            filesystem_volume_dir: default_filesystem_volume_dir(),
            lvm: None,
            zfs: None,
        }
    }
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(Path::new(path))
            .with_context(|| format!("reading config {path}"))?;
        let cfg: Config = serde_yaml::from_str(&contents).context("parsing config")?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.node_id.trim().is_empty() {
            anyhow::bail!("nodeId is required");
        }
        if self.listen_addr.parse::<std::net::SocketAddr>().is_err() {
            anyhow::bail!(
                "listen_addr '{}' is not a valid socket address",
                self.listen_addr
            );
        }
        if let Some(tls) = &self.tls {
            for (label, path) in [
                ("tls.ca_file", &tls.ca_file),
                ("tls.cert_file", &tls.cert_file),
                ("tls.key_file", &tls.key_file),
            ] {
                if !std::path::Path::new(path).exists() {
                    anyhow::bail!("{label} '{}' does not exist", path);
                }
            }
        }
        match &self.storage.backend {
            StorageBackendKind::Lvm => {
                if self.storage.lvm.is_none() {
                    anyhow::bail!("storage.lvm config is required when backend is 'lvm'");
                }
            }
            StorageBackendKind::Zfs => {
                if self.storage.zfs.is_none() {
                    anyhow::bail!("storage.zfs config is required when backend is 'zfs'");
                }
            }
            StorageBackendKind::Filesystem => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_defaults_to_filesystem_backend() {
        let parsed: Config = serde_yaml::from_str(
            r#"
nodeId: node-1
"#,
        )
        .expect("parse");
        assert!(matches!(
            parsed.storage.backend,
            StorageBackendKind::Filesystem
        ));
        assert_eq!(parsed.storage.image_cache_dir, "/var/lib/kcore/images");
        assert_eq!(
            parsed.storage.filesystem_volume_dir,
            "/var/lib/kcore/volumes"
        );
    }

    #[test]
    fn parses_lvm_storage_config() {
        let parsed: Config = serde_yaml::from_str(
            r#"
nodeId: node-1
storage:
  backend: lvm
  lvm:
    vgName: vg0
"#,
        )
        .expect("parse lvm");
        assert!(matches!(parsed.storage.backend, StorageBackendKind::Lvm));
        let lvm = parsed.storage.lvm.expect("lvm config");
        assert_eq!(lvm.vg_name, "vg0");
        assert_eq!(lvm.lv_prefix, "kcore-");
    }

    #[test]
    fn validate_rejects_lvm_without_config() {
        let cfg: Config = serde_yaml::from_str(
            r#"
nodeId: node-1
storage:
  backend: lvm
"#,
        )
        .expect("parse");
        let err = cfg
            .validate()
            .expect_err("should reject missing lvm config");
        assert!(err.to_string().contains("lvm"));
    }
}
