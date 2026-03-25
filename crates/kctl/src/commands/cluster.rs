use std::path::Path;

use anyhow::Result;
use crate::config::{self, Context};
use crate::pki;

pub fn create(
    config_path: &Path,
    controller: &str,
    certs_dir: &Path,
    context_name: &str,
    force: bool,
) -> Result<()> {
    let controller_host =
        pki::host_from_address(controller).map_err(|e| anyhow::anyhow!("invalid controller: {e}"))?;
    let pki_paths = pki::create_cluster_pki(certs_dir, &controller_host, force)
        .map_err(|e| anyhow::anyhow!("creating cluster PKI: {e}"))?;

    let mut cfg = config::load_config(config_path).unwrap_or_default();
    cfg.contexts.insert(
        context_name.to_string(),
        Context {
            controller: controller.to_string(),
            insecure: false,
            cert: Some(pki_paths.kctl_cert.display().to_string()),
            key: Some(pki_paths.kctl_key.display().to_string()),
            ca: Some(pki_paths.ca_cert.display().to_string()),
        },
    );
    cfg.current_context = Some(context_name.to_string());
    config::save_config(config_path, &cfg)?;

    println!("Cluster PKI generated in {}", pki_paths.certs_dir.display());
    println!(
        "Context '{}' saved to {}",
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
    fn create_writes_context_and_pki() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.yaml");
        let certs_dir = temp.path().join("certs");

        create(&config_path, "127.0.0.1:9090", &certs_dir, "test", false).expect("create cluster");

        let cfg = config::load_config(&config_path).expect("load config");
        let ctx = cfg.contexts.get("test").expect("context");
        assert_eq!(ctx.controller, "127.0.0.1:9090");
        assert!(Path::new(ctx.ca.as_ref().expect("ca")).exists());
        assert!(Path::new(ctx.cert.as_ref().expect("cert")).exists());
        assert!(Path::new(ctx.key.as_ref().expect("key")).exists());
        assert!(certs_dir.join("controller.crt").exists());
        assert!(certs_dir.join("controller.key").exists());
    }
}
