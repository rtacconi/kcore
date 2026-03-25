use anyhow::Result;
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

pub fn default_kcore_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".kcore")
}

pub fn default_certs_dir() -> PathBuf {
    default_kcore_dir().join("certs")
}

pub fn default_cluster_certs_dir(cluster_name: &str) -> PathBuf {
    default_kcore_dir().join(cluster_name)
}

pub fn resolve_install_certs_dir(config_path: &Path) -> Result<PathBuf, String> {
    let cfg = load_config(config_path).map_err(|e| format!("loading config: {e}"))?;
    let context_name = cfg
        .current_context
        .clone()
        .or_else(|| cfg.contexts.keys().next().cloned())
        .ok_or_else(|| {
            format!(
                "no cluster context configured in {} (run `kctl create cluster --context <name> ...` first)",
                config_path.display()
            )
        })?;
    let ctx = cfg
        .contexts
        .get(&context_name)
        .ok_or_else(|| format!("context '{context_name}' not found in {}", config_path.display()))?;

    if let Some(ca_path) = &ctx.ca {
        let parent = PathBuf::from(ca_path)
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| format!("invalid CA path in context '{context_name}': {ca_path}"))?;
        return Ok(parent);
    }

    // Preferred cluster-scoped layout: ~/.kcore/<cluster-name>/
    let cluster_scoped = default_cluster_certs_dir(&context_name);
    if cluster_scoped.exists() {
        return Ok(cluster_scoped);
    }

    // Backward compatibility for older flat layout.
    let flat = default_certs_dir();
    if flat.exists() {
        return Ok(flat);
    }

    Err(format!(
        "unable to resolve cert directory for context '{context_name}'. Tried {} and {}",
        default_cluster_certs_dir(&context_name).display(),
        default_certs_dir().display()
    ))
}

pub fn load_config(path: &Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let data = std::fs::read_to_string(path)?;
    let config: Config = serde_yaml::from_str(&data)?;
    Ok(config)
}

pub fn save_config(path: &Path, config: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_yaml::to_string(config)?;
    std::fs::write(path, data)?;
    Ok(())
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
    let default_certs = default_certs_dir();
    if let Some(addr) = controller_flag {
        let (cert, key, ca) = if insecure_flag {
            (None, None, None)
        } else {
            (
                Some(default_certs.join("kctl.crt").display().to_string()),
                Some(default_certs.join("kctl.key").display().to_string()),
                Some(default_certs.join("ca.crt").display().to_string()),
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
        cert: if ctx.insecure || insecure_flag {
            None
        } else {
            ctx.cert
                .clone()
                .or(Some(default_certs.join("kctl.crt").display().to_string()))
        },
        key: if ctx.insecure || insecure_flag {
            None
        } else {
            ctx.key
                .clone()
                .or(Some(default_certs.join("kctl.key").display().to_string()))
        },
        ca: if ctx.insecure || insecure_flag {
            None
        } else {
            ctx.ca
                .clone()
                .or(Some(default_certs.join("ca.crt").display().to_string()))
        },
    })
}

/// Resolve a node-agent address. The `--node` flag is required for direct node commands.
pub fn resolve_node(
    config_path: &Path,
    node_flag: &Option<String>,
    insecure_flag: bool,
) -> Result<ConnectionInfo, String> {
    let addr = node_flag
        .as_deref()
        .ok_or("--node flag is required for this command")?;

    let default_certs = default_certs_dir();
    let cfg = load_config(config_path).unwrap_or_default();
    let ctx = cfg.current_context().ok();

    let (cert, key, ca) = if insecure_flag || ctx.map(|c| c.insecure).unwrap_or(false) {
        (None, None, None)
    } else {
        let ctx_cert = ctx.and_then(|c| c.cert.clone());
        let ctx_key = ctx.and_then(|c| c.key.clone());
        let ctx_ca = ctx.and_then(|c| c.ca.clone());
        (
            ctx_cert.or(Some(default_certs.join("kctl.crt").display().to_string())),
            ctx_key.or(Some(default_certs.join("kctl.key").display().to_string())),
            ctx_ca.or(Some(default_certs.join("ca.crt").display().to_string())),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_controller_uses_flag_and_defaults_port() {
        let info = resolve_controller(Path::new("/nonexistent"), &Some("10.0.0.10".into()), true)
            .expect("resolve controller");
        assert_eq!(info.address, "10.0.0.10:9090");
        assert!(info.insecure);
        assert!(info.cert.is_none());
        assert!(info.key.is_none());
        assert!(info.ca.is_none());
    }

    #[test]
    fn resolve_node_requires_node_flag() {
        let result = resolve_node(Path::new("/nonexistent"), &None, true);
        match result {
            Ok(_) => panic!("expected missing --node error"),
            Err(err) => assert!(err.contains("--node flag is required")),
        }
    }

    #[test]
    fn resolve_node_defaults_port_and_uses_insecure_mode() {
        let info = resolve_node(Path::new("/nonexistent"), &Some("10.0.0.21".into()), true)
            .expect("resolve node");
        assert_eq!(info.address, "10.0.0.21:9091");
        assert!(info.insecure);
        assert!(info.cert.is_none());
        assert!(info.key.is_none());
        assert!(info.ca.is_none());
    }
}
