use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(
        rename = "current-context",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub current_context: Option<String>,
    #[serde(default)]
    pub contexts: HashMap<String, Context>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Context {
    #[serde(default)]
    pub controller: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub insecure: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca: Option<String>,
}

pub struct ConnectionInfo {
    pub address: String,
    pub insecure: bool,
    pub cert: Option<String>,
    pub key: Option<String>,
    pub ca: Option<String>,
}

pub fn default_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".kcore")
        .join("config")
}

pub fn load_config(path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let data = std::fs::read_to_string(path)?;
    let config: Config = serde_yaml::from_str(&data)?;
    Ok(config)
}

impl Config {
    pub fn current_context(&self) -> Result<&Context, String> {
        if let Some(name) = &self.current_context {
            self.contexts
                .get(name)
                .ok_or_else(|| format!("context '{name}' not found in config"))
        } else {
            self.contexts
                .values()
                .next()
                .ok_or_else(|| "no contexts configured".to_string())
        }
    }
}

fn normalize_address(addr: &str, default_port: u16) -> String {
    if addr.is_empty() {
        return String::new();
    }
    if addr.contains(':') {
        addr.to_string()
    } else {
        format!("{addr}:{default_port}")
    }
}

/// Resolve a controller address from CLI flags and config file.
/// Priority: flag > config > error.
pub fn resolve_controller(
    config_path: &Path,
    controller_flag: &Option<String>,
    insecure_flag: bool,
) -> Result<ConnectionInfo, String> {
    if let Some(addr) = controller_flag {
        let (cert, key, ca) = if insecure_flag {
            (None, None, None)
        } else {
            (
                Some("certs/controller.crt".to_string()),
                Some("certs/controller.key".to_string()),
                Some("certs/ca.crt".to_string()),
            )
        };
        return Ok(ConnectionInfo {
            address: normalize_address(addr, 9090),
            insecure: insecure_flag,
            cert,
            key,
            ca,
        });
    }

    let config = load_config(config_path).map_err(|e| format!("loading config: {e}"))?;
    let ctx = config.current_context().map_err(|e| {
        format!(
            "no controller configured: use --controller flag or create config at {}: {e}",
            config_path.display()
        )
    })?;

    Ok(ConnectionInfo {
        address: normalize_address(&ctx.controller, 9090),
        insecure: ctx.insecure || insecure_flag,
        cert: ctx
            .cert
            .clone()
            .or(Some("certs/controller.crt".to_string())),
        key: ctx.key.clone().or(Some("certs/controller.key".to_string())),
        ca: ctx.ca.clone().or(Some("certs/ca.crt".to_string())),
    })
}

/// Resolve a node-agent address. The `--node` flag is required for direct node commands.
pub fn resolve_node(
    node_flag: &Option<String>,
    insecure_flag: bool,
) -> Result<ConnectionInfo, String> {
    let addr = node_flag
        .as_deref()
        .ok_or("--node flag is required for this command")?;

    let (cert, key, ca) = if insecure_flag {
        (None, None, None)
    } else {
        (
            Some("certs/controller.crt".to_string()),
            Some("certs/controller.key".to_string()),
            Some("certs/ca.crt".to_string()),
        )
    };

    Ok(ConnectionInfo {
        address: normalize_address(addr, 9091),
        insecure: insecure_flag,
        cert,
        key,
        ca,
    })
}
