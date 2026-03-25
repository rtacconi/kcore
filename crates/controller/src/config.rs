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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsConfig {
    pub ca_file: String,
    pub cert_file: String,
    pub key_file: String,
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
        let contents = std::fs::read_to_string(Path::new(path))
            .with_context(|| format!("reading config {path}"))?;
        let cfg: Config = serde_yaml::from_str(&contents).context("parsing config")?;
        Ok(cfg)
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
}
