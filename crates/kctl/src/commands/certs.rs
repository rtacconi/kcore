use std::path::Path;

use anyhow::Result;

use crate::pki;

pub fn rotate(certs_dir: &Path, controller: &str) -> Result<()> {
    let controller_host = pki::host_from_address(controller)
        .map_err(|e| anyhow::anyhow!("invalid controller address: {e}"))?;

    pki::rotate_controller_cert(certs_dir, &controller_host)
        .map_err(|e| anyhow::anyhow!("rotating controller cert: {e}"))?;

    println!(
        "Controller certificate rotated with SAN: {controller_host}"
    );
    println!("  cert: {}", certs_dir.join("controller.crt").display());
    println!("  key:  {}", certs_dir.join("controller.key").display());
    println!();
    println!("Next steps:");
    println!("  1. Copy controller.crt and controller.key to the controller node");
    println!("  2. Restart kcore-controller (systemctl restart kcore-controller)");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_creates_new_controller_cert() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let certs_dir = tmp.path().join("certs");
        pki::create_cluster_pki(&certs_dir, "10.0.0.1", false).expect("create pki");

        let original_cert =
            std::fs::read_to_string(certs_dir.join("controller.crt")).expect("read cert");

        rotate(&certs_dir, "10.0.0.2:9090").expect("rotate");

        let new_cert =
            std::fs::read_to_string(certs_dir.join("controller.crt")).expect("read new cert");
        assert_ne!(original_cert, new_cert, "cert should have changed after rotation");

        let ca = std::fs::read_to_string(certs_dir.join("ca.crt")).expect("read ca");
        assert!(!ca.is_empty(), "CA cert should be unchanged");
    }

    #[test]
    fn rotate_fails_without_ca() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let certs_dir = tmp.path().join("empty-certs");
        std::fs::create_dir_all(&certs_dir).expect("mkdir");

        let result = rotate(&certs_dir, "10.0.0.1:9090");
        assert!(result.is_err());
    }
}
