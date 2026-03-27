//! Dashboard process configuration (controller gRPC target and TLS).

use anyhow::{Context, Result};
use std::path::PathBuf;

const ENV_CONTROLLER: &str = "KCORE_CONTROLLER";
const ENV_CONTROLLER_ALT: &str = "CONTROLLER_ADDR";
const ENV_CA: &str = "KCORE_CA_FILE";
const ENV_CERT: &str = "KCORE_CERT_FILE";
const ENV_KEY: &str = "KCORE_KEY_FILE";
const ENV_INSECURE: &str = "KCORE_INSECURE";
/// When set, used as the TLS certificate verification name (SNI / rustls `domain_name`).
/// Use when `KCORE_CONTROLLER` is `127.0.0.1` or an IP but the controller cert is issued for a hostname (e.g. `kvm-node`).
pub const ENV_TLS_DOMAIN: &str = "KCORE_TLS_DOMAIN";
const DEFAULT_ADDR: &str = "127.0.0.1:9090";

/// Paths and flags used to reach the controller over gRPC (same contract as `kctl`).
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    pub controller_addr: String,
    pub insecure: bool,
    pub ca: Option<PathBuf>,
    pub cert: Option<PathBuf>,
    pub key: Option<PathBuf>,
    pub tls_domain: Option<String>,
}

impl DashboardConfig {
    pub fn from_env() -> Result<Self> {
        let controller_addr = std::env::var(ENV_CONTROLLER)
            .or_else(|_| std::env::var(ENV_CONTROLLER_ALT))
            .unwrap_or_else(|_| DEFAULT_ADDR.to_string());

        let insecure = std::env::var(ENV_INSECURE)
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let ca = std::env::var(ENV_CA).ok().map(PathBuf::from);
        let cert = std::env::var(ENV_CERT).ok().map(PathBuf::from);
        let key = std::env::var(ENV_KEY).ok().map(PathBuf::from);
        let tls_domain = std::env::var(ENV_TLS_DOMAIN).ok();

        if !insecure && (ca.is_none() || cert.is_none() || key.is_none()) {
            anyhow::bail!(
                "mTLS required unless {}=1: set {}, {}, {}",
                ENV_INSECURE,
                ENV_CA,
                ENV_CERT,
                ENV_KEY
            );
        }

        Ok(Self {
            controller_addr,
            insecure,
            ca,
            cert,
            key,
            tls_domain,
        })
    }

    /// Development / tests: plaintext to localhost.
    pub fn local_insecure() -> Self {
        Self {
            controller_addr: DEFAULT_ADDR.to_string(),
            insecure: true,
            ca: None,
            cert: None,
            key: None,
            tls_domain: None,
        }
    }

    /// Plaintext gRPC to a specific `host:port` (integration tests).
    pub fn insecure_on(addr: impl Into<String>) -> Self {
        Self {
            controller_addr: addr.into(),
            insecure: true,
            ca: None,
            cert: None,
            key: None,
            tls_domain: None,
        }
    }

    /// Load PEM files for tonic TLS setup.
    pub fn tls_pem_strings(&self) -> Result<(String, String, String)> {
        let ca = self.ca.as_ref().context("missing CA path")?;
        let cert = self.cert.as_ref().context("missing cert path")?;
        let key = self.key.as_ref().context("missing key path")?;
        let ca_pem =
            std::fs::read_to_string(ca).with_context(|| format!("reading {}", ca.display()))?;
        let cert_pem =
            std::fs::read_to_string(cert).with_context(|| format!("reading {}", cert.display()))?;
        let key_pem =
            std::fs::read_to_string(key).with_context(|| format!("reading {}", key.display()))?;
        Ok((ca_pem, cert_pem, key_pem))
    }
}
