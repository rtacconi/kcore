use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
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
    #[serde(default)]
    pub controllers: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub insecure: bool,
    /// Hostname for TLS SNI and certificate name verification when the controller `address` is an
    /// IP (or when the server cert does not match the connection host).
    #[serde(
        rename = "tls-server-name",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tls_server_name: Option<String>,
    /// Inline base64-encoded PEM client certificate (preferred over file path).
    #[serde(rename = "cert-data", default, skip_serializing_if = "Option::is_none")]
    pub cert_data: Option<String>,
    /// Inline base64-encoded PEM client key (preferred over file path).
    #[serde(rename = "key-data", default, skip_serializing_if = "Option::is_none")]
    pub key_data: Option<String>,
    /// Inline base64-encoded PEM CA certificate (preferred over file path).
    #[serde(rename = "ca-data", default, skip_serializing_if = "Option::is_none")]
    pub ca_data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca: Option<String>,
}

/// Resolved TLS material: either decoded inline PEM or a file path to read at connect time.
#[derive(Debug)]
pub struct ConnectionInfo {
    pub address: String,
    pub addresses: Vec<String>,
    pub insecure: bool,
    /// Override for rustls/tonic TLS server name (SNI + cert verification). See `Context::tls_server_name`.
    pub tls_server_name: Option<String>,
    /// PEM string (already decoded from base64 inline data), or `None`.
    pub cert_pem: Option<String>,
    /// PEM string (already decoded from base64 inline data), or `None`.
    pub key_pem: Option<String>,
    /// PEM string (already decoded from base64 inline data), or `None`.
    pub ca_pem: Option<String>,
    /// File path fallback — only used when `*_pem` is `None`.
    pub cert: Option<String>,
    /// File path fallback — only used when `*_pem` is `None`.
    pub key: Option<String>,
    /// File path fallback — only used when `*_pem` is `None`.
    pub ca: Option<String>,
}

/// Decode a base64-encoded PEM string. Returns `Err` if the base64 is invalid.
fn decode_inline_pem(b64: &str, label: &str) -> Result<String, String> {
    let bytes = BASE64
        .decode(b64.trim())
        .map_err(|e| format!("invalid base64 in {label}: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("invalid UTF-8 in {label}: {e}"))
}

type PemTriple = (Option<String>, Option<String>, Option<String>);
type TlsFieldsTuple = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Resolve inline base64 data from a `Context` into decoded PEM strings.
fn resolve_inline_pems(ctx: &Context) -> Result<PemTriple, String> {
    let cert_pem = ctx
        .cert_data
        .as_deref()
        .map(|d| decode_inline_pem(d, "cert-data"))
        .transpose()?;
    let key_pem = ctx
        .key_data
        .as_deref()
        .map(|d| decode_inline_pem(d, "key-data"))
        .transpose()?;
    let ca_pem = ctx
        .ca_data
        .as_deref()
        .map(|d| decode_inline_pem(d, "ca-data"))
        .transpose()?;
    Ok((cert_pem, key_pem, ca_pem))
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
    let ctx = cfg.contexts.get(&context_name).ok_or_else(|| {
        format!(
            "context '{context_name}' not found in {}",
            config_path.display()
        )
    })?;

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
    std::fs::write(path, &data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
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

fn normalize_addresses(addrs: &[String], default_port: u16) -> Vec<String> {
    addrs
        .iter()
        .map(|s| normalize_address(s.trim(), default_port))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Resolve a controller address from CLI flags and config file.
/// Priority: flag > config > error.
///
/// Inline cert data (`cert-data`, `key-data`, `ca-data`) takes precedence over
/// file paths. No silent fallback to `~/.kcore/certs/` — if no TLS credentials
/// are configured, this returns a clear error.
pub fn resolve_controller(
    config_path: &Path,
    controller_flags: &[String],
    insecure_flag: bool,
    tls_server_name_flag: Option<&str>,
) -> Result<ConnectionInfo, String> {
    let tls_from_cli = tls_server_name_flag
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(std::string::ToString::to_string);

    // When --controller flag is used directly, we still need certs from the current context.
    if !controller_flags.is_empty() {
        let addresses = normalize_addresses(controller_flags, 9090);
        if addresses.is_empty() {
            return Err("no valid --controller endpoints provided".to_string());
        }
        if insecure_flag {
            return Ok(ConnectionInfo {
                address: addresses[0].clone(),
                addresses,
                insecure: true,
                tls_server_name: tls_from_cli,
                cert_pem: None,
                key_pem: None,
                ca_pem: None,
                cert: None,
                key: None,
                ca: None,
            });
        }
        // Try to resolve certs from config context if available.
        // `unwrap_or_default()` would silently treat a corrupt config as
        // "no contexts" and then bail with the misleading "no TLS
        // credentials" error below — operators would think their cluster
        // was missing certs when really their config file was unreadable.
        let config = load_config(config_path)
            .map_err(|e| format!("loading config at {}: {e:#}", config_path.display()))?;
        let ctx = config.current_context().ok();
        let (cert_pem, key_pem, ca_pem, cert, key, ca) = if let Some(ctx) = ctx {
            resolve_tls_fields(ctx)?
        } else {
            (None, None, None, None, None, None)
        };
        if cert_pem.is_none() && cert.is_none() {
            return Err(
                "no TLS credentials available — run `kctl create cluster` first or use -k for insecure mode".to_string(),
            );
        }
        return Ok(ConnectionInfo {
            address: addresses[0].clone(),
            addresses,
            insecure: false,
            tls_server_name: tls_from_cli,
            cert_pem,
            key_pem,
            ca_pem,
            cert,
            key,
            ca,
        });
    }

    let config = load_config(config_path).map_err(|e| format!("loading config: {e}"))?;
    let context_name = config
        .current_context
        .as_deref()
        .or_else(|| config.contexts.keys().next().map(|s| s.as_str()))
        .unwrap_or("<none>");
    let ctx = config.current_context().map_err(|e| {
        format!(
            "no controller configured: use --controller flag or create config at {}: {e}",
            config_path.display()
        )
    })?;

    let addresses = if !ctx.controllers.is_empty() {
        normalize_addresses(&ctx.controllers, 9090)
    } else {
        let normalized = normalize_address(&ctx.controller, 9090);
        if normalized.is_empty() {
            Vec::new()
        } else {
            vec![normalized]
        }
    };
    if addresses.is_empty() {
        return Err("no controller endpoints configured in current context".to_string());
    }

    let tls_server_name = tls_from_cli.or_else(|| {
        ctx.tls_server_name
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    });

    let is_insecure = ctx.insecure || insecure_flag;
    if is_insecure {
        return Ok(ConnectionInfo {
            address: addresses[0].clone(),
            addresses,
            insecure: true,
            tls_server_name,
            cert_pem: None,
            key_pem: None,
            ca_pem: None,
            cert: None,
            key: None,
            ca: None,
        });
    }

    let (cert_pem, key_pem, ca_pem, cert, key, ca) = resolve_tls_fields(ctx)?;
    if cert_pem.is_none() && cert.is_none() {
        return Err(format!(
            "no TLS credentials in context '{}' — run `kctl create cluster` or set cert/key/ca paths",
            context_name
        ));
    }

    Ok(ConnectionInfo {
        address: addresses[0].clone(),
        addresses,
        insecure: false,
        tls_server_name,
        cert_pem,
        key_pem,
        ca_pem,
        cert,
        key,
        ca,
    })
}

/// Extract TLS material from a `Context`: inline data (decoded) or file paths.
/// Inline data takes precedence per field — a config may mix inline `ca_data`
/// with file-backed `cert`/`key`.
fn resolve_tls_fields(ctx: &Context) -> Result<TlsFieldsTuple, String> {
    let (cert_pem, key_pem, ca_pem) = resolve_inline_pems(ctx)?;
    let cert_path = if ctx.cert_data.is_some() {
        None
    } else {
        ctx.cert.clone()
    };
    let key_path = if ctx.key_data.is_some() {
        None
    } else {
        ctx.key.clone()
    };
    let ca_path = if ctx.ca_data.is_some() {
        None
    } else {
        ctx.ca.clone()
    };

    let has_cert = cert_pem.is_some() || cert_path.is_some();
    let has_key = key_pem.is_some() || key_path.is_some();
    let has_ca = ca_pem.is_some() || ca_path.is_some();
    let present_count = [has_cert, has_key, has_ca].iter().filter(|&&b| b).count();
    if present_count > 0 && present_count < 3 {
        return Err(
            "incomplete TLS config: secure mode requires all of CA, client certificate, and client key \
             (either inline *_data or file paths)"
                .to_string(),
        );
    }

    Ok((cert_pem, key_pem, ca_pem, cert_path, key_path, ca_path))
}

/// Resolve a node-agent address. The `--node` flag is required for direct node commands.
pub fn resolve_node(
    config_path: &Path,
    node_flag: &Option<String>,
    insecure_flag: bool,
    tls_server_name_flag: Option<String>,
) -> Result<ConnectionInfo, String> {
    let addr = node_flag
        .as_deref()
        .ok_or("--node flag is required for this command")?;

    // Don't silently fall back to "no contexts" when the file exists but
    // fails to parse — TLS material from disk would be ignored and the
    // caller might silently connect insecurely. `load_config` already
    // returns `Ok(Config::default())` when the file simply does not exist,
    // so any `Err` here is a real parse / I/O failure worth surfacing.
    let cfg = load_config(config_path)
        .map_err(|e| format!("loading config at {}: {e:#}", config_path.display()))?;
    let ctx = cfg.current_context().ok();

    let is_insecure = insecure_flag || ctx.map(|c| c.insecure).unwrap_or(false);
    let tls_from_cli = tls_server_name_flag
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let tls_server_name = tls_from_cli.or_else(|| {
        ctx.and_then(|c| {
            c.tls_server_name
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
    });

    if is_insecure {
        return Ok(ConnectionInfo {
            address: normalize_address(addr, 9091),
            addresses: vec![normalize_address(addr, 9091)],
            insecure: true,
            tls_server_name,
            cert_pem: None,
            key_pem: None,
            ca_pem: None,
            cert: None,
            key: None,
            ca: None,
        });
    }

    let (cert_pem, key_pem, ca_pem, cert, key, ca) = if let Some(ctx) = ctx {
        resolve_tls_fields(ctx)?
    } else {
        (None, None, None, None, None, None)
    };

    Ok(ConnectionInfo {
        address: normalize_address(addr, 9091),
        addresses: vec![normalize_address(addr, 9091)],
        insecure: false,
        tls_server_name,
        cert_pem,
        key_pem,
        ca_pem,
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
        let info = resolve_controller(Path::new("/nonexistent"), &["10.0.0.10".into()], true, None)
            .expect("resolve controller");
        assert_eq!(info.address, "10.0.0.10:9090");
        assert_eq!(info.addresses, vec!["10.0.0.10:9090"]);
        assert!(info.insecure);
        assert!(info.tls_server_name.is_none());
        assert!(info.cert.is_none());
        assert!(info.key.is_none());
        assert!(info.ca.is_none());
        assert!(info.cert_pem.is_none());
        assert!(info.key_pem.is_none());
        assert!(info.ca_pem.is_none());
    }

    #[test]
    fn resolve_node_requires_node_flag() {
        let result = resolve_node(Path::new("/nonexistent"), &None, true, None);
        match result {
            Ok(_) => panic!("expected missing --node error"),
            Err(err) => assert!(err.contains("--node flag is required")),
        }
    }

    #[test]
    fn resolve_node_surfaces_corrupt_config_instead_of_silently_dropping_creds() {
        // Regression: a corrupt config file used to be flattened to an empty
        // Config (so TLS material from the file was silently dropped). The
        // operator would either connect insecurely or get a confusing
        // "no TLS credentials" error instead of "your config is corrupt".
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        std::fs::write(&config_path, "}{this is not yaml").expect("write broken config");

        let err = resolve_node(
            &config_path,
            &Some("10.0.0.21".into()),
            true, // even with insecure flag, we still must reject a corrupt config
            None,
        )
        .expect_err("must surface parse error");
        assert!(
            err.contains("loading config"),
            "should mention the load failure, got: {err}"
        );
    }

    #[test]
    fn resolve_controller_surfaces_corrupt_config_instead_of_no_tls_message() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        std::fs::write(&config_path, "}{this is not yaml").expect("write broken config");

        let err = resolve_controller(&config_path, &["192.168.1.1".into()], false, None)
            .expect_err("must surface parse error");
        assert!(
            err.contains("loading config"),
            "should mention the load failure, got: {err}"
        );
    }

    #[test]
    fn resolve_node_defaults_port_and_uses_insecure_mode() {
        let info = resolve_node(
            Path::new("/nonexistent"),
            &Some("10.0.0.21".into()),
            true,
            None,
        )
        .expect("resolve node");
        assert_eq!(info.address, "10.0.0.21:9091");
        assert_eq!(info.addresses, vec!["10.0.0.21:9091"]);
        assert!(info.insecure);
        assert!(info.cert.is_none());
        assert!(info.key.is_none());
        assert!(info.ca.is_none());
        assert!(info.cert_pem.is_none());
    }

    #[test]
    fn resolve_controller_accepts_multiple_flag_endpoints() {
        let info = resolve_controller(
            Path::new("/nonexistent"),
            &["10.0.0.10".into(), "10.0.0.11:9090".into()],
            true,
            None,
        )
        .expect("resolve controller");
        assert_eq!(info.address, "10.0.0.10:9090");
        assert_eq!(info.addresses, vec!["10.0.0.10:9090", "10.0.0.11:9090"]);
    }

    #[test]
    fn resolve_controller_tls_server_name_from_cli() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let ca_b64 = BASE64.encode(b"---CA---");
        let cert_b64 = BASE64.encode(b"---CERT---");
        let key_b64 = BASE64.encode(b"---KEY---");
        let cfg = Config {
            current_context: Some("ctx".into()),
            contexts: {
                let mut m = HashMap::new();
                m.insert(
                    "ctx".into(),
                    Context {
                        controller: "192.168.1.1:9090".into(),
                        ca_data: Some(ca_b64),
                        cert_data: Some(cert_b64),
                        key_data: Some(key_b64),
                        ..Default::default()
                    },
                );
                m
            },
        };
        save_config(&config_path, &cfg).expect("save");
        let info = resolve_controller(
            &config_path,
            &["192.168.1.1".into()],
            false,
            Some("controller.example.com"),
        )
        .expect("resolve");
        assert_eq!(
            info.tls_server_name.as_deref(),
            Some("controller.example.com")
        );
    }

    #[test]
    fn resolve_controller_inline_data_takes_precedence() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let ca_b64 = BASE64.encode(b"---CA PEM---");
        let cert_b64 = BASE64.encode(b"---CERT PEM---");
        let key_b64 = BASE64.encode(b"---KEY PEM---");
        let cfg = Config {
            current_context: Some("test".into()),
            contexts: {
                let mut m = HashMap::new();
                m.insert(
                    "test".into(),
                    Context {
                        controller: "10.0.0.1:9090".into(),
                        controllers: vec!["10.0.0.1:9090".into()],
                        ca_data: Some(ca_b64),
                        cert_data: Some(cert_b64),
                        key_data: Some(key_b64),
                        cert: Some("/stale/path/kctl.crt".into()),
                        key: Some("/stale/path/kctl.key".into()),
                        ca: Some("/stale/path/ca.crt".into()),
                        ..Default::default()
                    },
                );
                m
            },
        };
        save_config(&config_path, &cfg).expect("save");
        let info = resolve_controller(&config_path, &[], false, None).expect("resolve");
        assert_eq!(info.ca_pem.as_deref(), Some("---CA PEM---"));
        assert_eq!(info.cert_pem.as_deref(), Some("---CERT PEM---"));
        assert_eq!(info.key_pem.as_deref(), Some("---KEY PEM---"));
        assert!(
            info.cert.is_none(),
            "file path must not be set when inline data is present"
        );
        assert!(info.key.is_none());
        assert!(info.ca.is_none());
    }

    #[test]
    fn resolve_controller_no_creds_returns_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let cfg = Config {
            current_context: Some("empty".into()),
            contexts: {
                let mut m = HashMap::new();
                m.insert(
                    "empty".into(),
                    Context {
                        controller: "10.0.0.1:9090".into(),
                        controllers: vec!["10.0.0.1:9090".into()],
                        ..Default::default()
                    },
                );
                m
            },
        };
        save_config(&config_path, &cfg).expect("save");
        let err = resolve_controller(&config_path, &[], false, None).expect_err("should fail");
        assert!(
            err.contains("no TLS credentials"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_controller_file_paths_still_work() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let cfg = Config {
            current_context: Some("files".into()),
            contexts: {
                let mut m = HashMap::new();
                m.insert(
                    "files".into(),
                    Context {
                        controller: "10.0.0.1:9090".into(),
                        controllers: vec!["10.0.0.1:9090".into()],
                        cert: Some("/path/to/kctl.crt".into()),
                        key: Some("/path/to/kctl.key".into()),
                        ca: Some("/path/to/ca.crt".into()),
                        ..Default::default()
                    },
                );
                m
            },
        };
        save_config(&config_path, &cfg).expect("save");
        let info = resolve_controller(&config_path, &[], false, None).expect("resolve");
        assert!(info.cert_pem.is_none());
        assert_eq!(info.cert.as_deref(), Some("/path/to/kctl.crt"));
        assert_eq!(info.key.as_deref(), Some("/path/to/kctl.key"));
        assert_eq!(info.ca.as_deref(), Some("/path/to/ca.crt"));
    }

    #[test]
    fn config_roundtrip_with_inline_data() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let ca_b64 = BASE64.encode(b"ca-pem-content");
        let cert_b64 = BASE64.encode(b"cert-pem-content");
        let key_b64 = BASE64.encode(b"key-pem-content");
        let cfg = Config {
            current_context: Some("rt".into()),
            contexts: {
                let mut m = HashMap::new();
                m.insert(
                    "rt".into(),
                    Context {
                        controller: "1.2.3.4:9090".into(),
                        ca_data: Some(ca_b64.clone()),
                        cert_data: Some(cert_b64.clone()),
                        key_data: Some(key_b64.clone()),
                        ..Default::default()
                    },
                );
                m
            },
        };
        save_config(&config_path, &cfg).expect("save");
        let loaded = load_config(&config_path).expect("load");
        let ctx = loaded.contexts.get("rt").expect("context");
        assert_eq!(ctx.ca_data.as_deref(), Some(ca_b64.as_str()));
        assert_eq!(ctx.cert_data.as_deref(), Some(cert_b64.as_str()));
        assert_eq!(ctx.key_data.as_deref(), Some(key_b64.as_str()));
    }
}
