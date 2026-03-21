use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[allow(dead_code)]
    pub tls: Option<TlsConfig>,
    pub default_network: NetworkConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
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
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(Path::new(path))
            .map_err(|e| format!("reading config {path}: {e}"))?;
        let cfg: Config =
            serde_yaml::from_str(&contents).map_err(|e| format!("parsing config: {e}"))?;
        Ok(cfg)
    }
}
