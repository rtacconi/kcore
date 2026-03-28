use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    pub tls: Option<TlsConfig>,
    pub default_network: NetworkConfig,
    /// When set, mutating RPCs append JSON envelopes to `replication_outbox` for future peer sync.
    #[serde(default)]
    pub replication: Option<ReplicationConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicationConfig {
    #[serde(default)]
    pub controller_id: String,
    #[serde(default = "default_dc_id")]
    pub dc_id: String,
    #[serde(default)]
    pub peers: Vec<String>,
}

fn default_dc_id() -> String {
    "DC1".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsConfig {
    pub ca_file: String,
    pub cert_file: String,
    pub key_file: String,
    #[serde(default)]
    pub sub_ca_cert_file: Option<String>,
    #[serde(default)]
    pub sub_ca_key_file: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
    pub gateway_interface: String,
    pub external_ip: String,
    pub gateway_ip: String,
    #[serde(default = "default_netmask")]
    pub internal_netmask: String,
}

fn default_listen_addr() -> String {
    "0.0.0.0:9090".to_string()
}

fn default_db_path() -> String {
    "/var/lib/kcore/controller.db".to_string()
}

fn default_netmask() -> String {
    "255.255.255.0".to_string()
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        crate::path_safety::assert_safe_path(path, "config file path")?;
        let contents = std::fs::read_to_string(Path::new(path))
            .with_context(|| format!("reading config {path}"))?;
        let cfg: Config = serde_yaml::from_str(&contents).context("parsing config")?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        crate::path_safety::assert_safe_path(&self.db_path, "dbPath")?;
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
                crate::path_safety::assert_safe_path(path, label)?;
                if !std::path::Path::new(path).exists() {
                    anyhow::bail!("{label} '{}' does not exist", path);
                }
            }
            if let Some(p) = &tls.sub_ca_cert_file {
                crate::path_safety::assert_safe_path(p, "tls.sub_ca_cert_file")?;
            }
            if let Some(p) = &tls.sub_ca_key_file {
                crate::path_safety::assert_safe_path(p, "tls.sub_ca_key_file")?;
            }
        }
        if self.default_network.gateway_interface.trim().is_empty() {
            anyhow::bail!("defaultNetwork.gatewayInterface is required");
        }
        if self.default_network.external_ip.trim().is_empty() {
            anyhow::bail!("defaultNetwork.externalIp is required");
        }
        if self.default_network.gateway_ip.trim().is_empty() {
            anyhow::bail!("defaultNetwork.gatewayIp is required");
        }
        if let Some(replication) = &self.replication {
            if replication.dc_id.trim().is_empty() {
                anyhow::bail!("replication.dcId must not be empty");
            }
            if !replication.peers.is_empty() && replication.controller_id.trim().is_empty() {
                anyhow::bail!("replication.controllerId is required when replication.peers is set");
            }
            if replication.peers.iter().any(|p| p.trim().is_empty()) {
                anyhow::bail!("replication.peers must not contain empty endpoints");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config_path(name: &str) -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("kcore-controller-{name}-{ts}.yaml"))
    }

    #[test]
    fn load_applies_defaults_for_optional_fields() {
        let path = temp_config_path("defaults");
        std::fs::write(
            &path,
            r#"
defaultNetwork:
  gatewayInterface: eno1
  externalIp: 203.0.113.10
  gatewayIp: 10.0.0.1
"#,
        )
        .expect("write config");

        let cfg = Config::load(path.to_str().expect("path str")).expect("load config");
        assert_eq!(cfg.listen_addr, "0.0.0.0:9090");
        assert_eq!(cfg.db_path, "/var/lib/kcore/controller.db");
        assert_eq!(cfg.default_network.internal_netmask, "255.255.255.0");
        assert!(cfg.replication.is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_parses_replication_section() {
        let path = temp_config_path("repl");
        std::fs::write(
            &path,
            r#"
defaultNetwork:
  gatewayInterface: eno1
  externalIp: 203.0.113.10
  gatewayIp: 10.0.0.1
replication:
  controllerId: ctrl-a
  dcId: DC2
  peers:
    - 10.0.0.11:9090
"#,
        )
        .expect("write config");

        let cfg = Config::load(path.to_str().expect("path str")).expect("load config");
        let rep = cfg.replication.expect("replication");
        assert_eq!(rep.controller_id, "ctrl-a");
        assert_eq!(rep.dc_id, "DC2");
        assert_eq!(rep.peers, vec!["10.0.0.11:9090"]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_rejects_replication_peers_without_controller_id() {
        let path = temp_config_path("repl-invalid");
        std::fs::write(
            &path,
            r#"
defaultNetwork:
  gatewayInterface: eno1
  externalIp: 203.0.113.10
  gatewayIp: 10.0.0.1
replication:
  dcId: DC1
  peers:
    - 10.0.0.11:9090
"#,
        )
        .expect("write config");
        let err = Config::load(path.to_str().expect("path str")).expect_err("must fail");
        assert!(err
            .to_string()
            .contains("replication.controllerId is required"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_returns_error_for_invalid_yaml() {
        let path = temp_config_path("invalid");
        std::fs::write(&path, "defaultNetwork: [").expect("write invalid config");
        let err = Config::load(path.to_str().expect("path str")).expect_err("invalid yaml");
        assert!(err.to_string().contains("parsing config"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_rejects_parent_dir_in_config_file_argument() {
        let err = Config::load("../nonexistent-kcore-config.yaml").expect_err("traversal");
        let s = format!("{err:#}");
        assert!(
            s.contains("config file path") && s.contains(".."),
            "unexpected error: {s}"
        );
    }

    #[test]
    fn load_rejects_parent_dir_in_db_path_field() {
        let path = temp_config_path("bad-db");
        std::fs::write(
            &path,
            r#"
dbPath: ../../../tmp/evil.db
defaultNetwork:
  gatewayInterface: eno1
  externalIp: 203.0.113.10
  gatewayIp: 10.0.0.1
"#,
        )
        .expect("write config");
        let err = Config::load(path.to_str().expect("path str")).expect_err("bad db path");
        let s = format!("{err:#}");
        assert!(
            s.contains("dbPath") && s.contains(".."),
            "unexpected error: {s}"
        );
        let _ = std::fs::remove_file(path);
    }
}
