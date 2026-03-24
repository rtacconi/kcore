use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub node_id: String,
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[allow(dead_code)]
    pub controller_addr: Option<String>,
    pub tls: Option<TlsConfig>,
    #[serde(default = "default_vm_socket_dir")]
    pub vm_socket_dir: String,
    #[serde(default = "default_nix_config_path")]
    pub nix_config_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsConfig {
    pub ca_file: String,
    pub cert_file: String,
    pub key_file: String,
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

impl Config {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(Path::new(path))
            .map_err(|e| format!("reading config {path}: {e}"))?;
        let cfg: Config =
            serde_yaml::from_str(&contents).map_err(|e| format!("parsing config: {e}"))?;
        Ok(cfg)
    }
}
