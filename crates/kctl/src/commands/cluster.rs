use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Deserialize;

use crate::config::{self, Context};
use crate::pki;
use anyhow::{Context as _, Result};

#[derive(Debug, Deserialize)]
struct ClusterManifest {
    metadata: ClusterMetadata,
    spec: ClusterSpec,
}

#[derive(Debug, Deserialize)]
struct ClusterMetadata {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClusterSpec {
    controller: String,
    #[serde(alias = "certs_dir")]
    certs_dir: Option<String>,
    #[serde(default)]
    force: bool,
}

pub fn create_from_manifest(file: &str, config_path: &Path) -> Result<()> {
    let content = std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?;
    let manifest: ClusterManifest = serde_yaml::from_str(&content)
        .with_context(|| format!("parsing Cluster manifest {file}"))?;

    let context_name = manifest.metadata.name;
    let controller = &manifest.spec.controller;
    let certs_dir = manifest
        .spec
        .certs_dir
        .map(|p| {
            if let Some(rest) = p.strip_prefix("~/") {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(rest)
            } else {
                PathBuf::from(&p)
            }
        })
        .unwrap_or_else(|| config::default_cluster_certs_dir(&context_name));

    create(
        config_path,
        controller,
        &certs_dir,
        &context_name,
        manifest.spec.force,
    )
}

pub fn create(
    config_path: &Path,
    controller: &str,
    certs_dir: &Path,
    context_name: &str,
    force: bool,
) -> Result<()> {
    if !force {
        if let Ok(existing_cfg) = config::load_config(config_path) {
            if let Some(ctx) = existing_cfg.contexts.get(context_name) {
                let has_creds = ctx.ca_data.is_some()
                    || ctx.ca.is_some()
                    || ctx.cert_data.is_some()
                    || ctx.cert.is_some()
                    || ctx.key_data.is_some()
                    || ctx.key.is_some();
                if has_creds {
                    anyhow::bail!(
                        "context '{}' already has TLS credentials in {}. \
                         Re-running `create cluster` would generate a new CA and \
                         break connections to any controllers using the current trust root. \
                         Use --force to overwrite deliberately.",
                        context_name,
                        config_path.display()
                    );
                }
            }
        }
    }

    let controller_host = pki::host_from_address(controller)
        .map_err(|e| anyhow::anyhow!("invalid controller: {e}"))?;
    let pki_paths = pki::create_cluster_pki(certs_dir, &controller_host, force)
        .map_err(|e| anyhow::anyhow!("creating cluster PKI: {e}"))?;

    let ca_pem = std::fs::read_to_string(&pki_paths.ca_cert)?;
    let cert_pem = std::fs::read_to_string(&pki_paths.kctl_cert)?;
    let key_pem = std::fs::read_to_string(&pki_paths.kctl_key)?;

    // CRITICAL: Never use `unwrap_or_default()` here. `load_config` only
    // returns `Ok(Config::default())` when the file does not exist; if the
    // file exists but fails to parse (corrupt YAML, unsupported merge keys,
    // permission error), an `_or_default()` would silently replace the user's
    // entire kubeconfig with an empty one and immediately overwrite it on
    // disk in `save_config()` — wiping every other context.
    let mut cfg = config::load_config(config_path).with_context(|| {
        format!(
            "loading existing config at {} (refusing to overwrite a config that failed to parse)",
            config_path.display()
        )
    })?;
    cfg.contexts.insert(
        context_name.to_string(),
        Context {
            controller: controller.to_string(),
            controllers: vec![controller.to_string()],
            insecure: false,
            tls_server_name: None,
            ca_data: Some(BASE64.encode(ca_pem.as_bytes())),
            cert_data: Some(BASE64.encode(cert_pem.as_bytes())),
            key_data: Some(BASE64.encode(key_pem.as_bytes())),
            cert: None,
            key: None,
            ca: None,
        },
    );
    cfg.current_context = Some(context_name.to_string());
    config::save_config(config_path, &cfg)?;

    println!("Cluster PKI generated in {}", pki_paths.certs_dir.display());
    println!(
        "Context '{}' saved to {} (cert data embedded inline)",
        context_name,
        config_path.display()
    );
    println!(
        "Controller certificate: {}",
        pki_paths.controller_cert.display()
    );
    println!("Controller key: {}", pki_paths.controller_key.display());
    println!("CA certificate: {}", pki_paths.ca_cert.display());
    println!("mTLS is now the default for this context.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_writes_context_with_inline_data() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let certs_dir = temp.path().join("certs");

        create(&config_path, "127.0.0.1:9090", &certs_dir, "test", false).expect("create cluster");

        let cfg = config::load_config(&config_path).expect("load config");
        let ctx = cfg.contexts.get("test").expect("context");
        assert_eq!(ctx.controller, "127.0.0.1:9090");

        assert!(ctx.ca_data.is_some(), "ca-data must be embedded inline");
        assert!(ctx.cert_data.is_some(), "cert-data must be embedded inline");
        assert!(ctx.key_data.is_some(), "key-data must be embedded inline");

        assert!(
            ctx.ca.is_none(),
            "file path should not be set when inline data is used"
        );
        assert!(ctx.cert.is_none());
        assert!(ctx.key.is_none());

        let ca_decoded = BASE64
            .decode(ctx.ca_data.as_ref().unwrap().trim())
            .expect("decode ca");
        let ca_str = String::from_utf8(ca_decoded).expect("utf8");
        assert!(
            ca_str.contains("BEGIN CERTIFICATE"),
            "decoded ca must be PEM"
        );

        assert!(certs_dir.join("controller.crt").exists());
        assert!(certs_dir.join("controller.key").exists());
    }

    #[test]
    fn create_refuses_overwrite_without_force() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let certs_dir = temp.path().join("certs");

        create(&config_path, "127.0.0.1:9090", &certs_dir, "test", false).expect("first create");
        let err = create(&config_path, "127.0.0.1:9090", &certs_dir, "test", false)
            .expect_err("should refuse");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("already has TLS credentials"),
            "should warn about existing trust root: {msg}"
        );
    }

    #[test]
    fn create_refuses_overwrite_of_context_with_inline_data() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let certs_dir_1 = temp.path().join("certs1");
        let certs_dir_2 = temp.path().join("certs2");

        create(&config_path, "127.0.0.1:9090", &certs_dir_1, "prod", false).expect("first create");

        let err = create(&config_path, "10.0.0.1:9090", &certs_dir_2, "prod", false)
            .expect_err("should refuse");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("already has TLS credentials") && msg.contains("--force"),
            "should mention --force to override: {msg}"
        );
    }

    #[test]
    fn create_with_force_overwrites_context() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let certs_dir_1 = temp.path().join("certs1");
        let certs_dir_2 = temp.path().join("certs2");

        create(&config_path, "127.0.0.1:9090", &certs_dir_1, "prod", false).expect("first create");
        create(&config_path, "10.0.0.1:9090", &certs_dir_2, "prod", true)
            .expect("force overwrite should succeed");

        let cfg = config::load_config(&config_path).expect("load config");
        let ctx = cfg.contexts.get("prod").expect("context");
        assert_eq!(ctx.controller, "10.0.0.1:9090");
    }

    #[test]
    fn create_from_manifest_parses_yaml() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let manifest_path = temp.path().join("cluster.yaml");
        let certs_dir = temp.path().join("my-certs");

        std::fs::write(
            &manifest_path,
            format!(
                "kind: Cluster\nmetadata:\n  name: staging\nspec:\n  controller: 127.0.0.1:9090\n  certsDir: {}\n",
                certs_dir.display()
            ),
        )
        .expect("write manifest");

        create_from_manifest(manifest_path.to_str().unwrap(), &config_path)
            .expect("create_from_manifest");

        let cfg = config::load_config(&config_path).expect("load config");
        let ctx = cfg.contexts.get("staging").expect("context");
        assert_eq!(ctx.controller, "127.0.0.1:9090");
        assert!(ctx.ca_data.is_some());
        assert!(certs_dir.join("controller.crt").exists());
    }

    #[test]
    fn create_from_manifest_defaults_certs_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let manifest_path = temp.path().join("cluster.yaml");

        std::fs::write(
            &manifest_path,
            "kind: Cluster\nmetadata:\n  name: testctx\nspec:\n  controller: 127.0.0.1:9090\n  force: true\n",
        )
        .expect("write manifest");

        create_from_manifest(manifest_path.to_str().unwrap(), &config_path)
            .expect("create_from_manifest");

        let cfg = config::load_config(&config_path).expect("load config");
        assert!(cfg.contexts.contains_key("testctx"));
    }

    #[test]
    fn create_refuses_when_existing_config_is_unparseable() {
        // Regression: a corrupt config file used to be silently treated as
        // an empty Config, then overwritten by `save_config` — wiping every
        // pre-existing context. The fix is to fail loudly so the operator
        // can repair (or back up) the file before continuing.
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let certs_dir = temp.path().join("certs");

        std::fs::write(&config_path, "this: is: not valid: yaml: at all\n  - [\n")
            .expect("write broken config");

        let err = create(&config_path, "127.0.0.1:9090", &certs_dir, "fresh", true)
            .expect_err("must refuse to overwrite a corrupt config");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("loading existing config"),
            "should mention the load failure: {msg}"
        );

        let on_disk = std::fs::read_to_string(&config_path).expect("file must still exist");
        assert!(
            on_disk.contains("not valid"),
            "the corrupt config must be left untouched, got: {on_disk}"
        );
    }

    #[test]
    fn create_from_manifest_supports_snake_case() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let manifest_path = temp.path().join("cluster.yaml");
        let certs_dir = temp.path().join("snake-certs");

        std::fs::write(
            &manifest_path,
            format!(
                "kind: Cluster\nmetadata:\n  name: snaketest\nspec:\n  controller: 127.0.0.1:9090\n  certs_dir: {}\n",
                certs_dir.display()
            ),
        )
        .expect("write manifest");

        create_from_manifest(manifest_path.to_str().unwrap(), &config_path)
            .expect("create_from_manifest with snake_case");

        let cfg = config::load_config(&config_path).expect("load config");
        assert!(cfg.contexts.contains_key("snaketest"));
    }
}
