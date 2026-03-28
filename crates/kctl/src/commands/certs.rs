use std::path::Path;

use anyhow::Result;

use crate::config::ConnectionInfo;
use crate::{client, pki};

pub async fn rotate(
    certs_dir: &Path,
    controller: &str,
    info: Option<&ConnectionInfo>,
) -> Result<()> {
    let controller_host = pki::host_from_address(controller)
        .map_err(|e| anyhow::anyhow!("invalid controller address: {e}"))?;

    pki::rotate_controller_cert(certs_dir, &controller_host)
        .map_err(|e| anyhow::anyhow!("rotating controller cert: {e}"))?;

    println!("Controller certificate rotated with SAN: {controller_host}");
    println!("  cert: {}", certs_dir.join("controller.crt").display());
    println!("  key:  {}", certs_dir.join("controller.key").display());

    if let Some(info) = info {
        let cert_pem = std::fs::read_to_string(certs_dir.join("controller.crt"))
            .map_err(|e| anyhow::anyhow!("reading new controller cert: {e}"))?;
        let key_pem = std::fs::read_to_string(certs_dir.join("controller.key"))
            .map_err(|e| anyhow::anyhow!("reading new controller key: {e}"))?;

        let mut ctrl = client::controller_client(info).await?;
        let resp = ctrl
            .reload_tls(client::controller_proto::ReloadTlsRequest { cert_pem, key_pem })
            .await?
            .into_inner();

        if resp.success {
            println!("TLS reload pushed to controller: {}", resp.message);
        } else {
            anyhow::bail!("Controller rejected TLS reload: {}", resp.message);
        }
    } else {
        println!();
        println!("Next steps:");
        println!("  1. Copy controller.crt and controller.key to the controller node");
        println!("  2. Restart kcore-controller (systemctl restart kcore-controller)");
    }

    Ok(())
}

pub async fn rotate_sub_ca(certs_dir: &Path, info: &ConnectionInfo) -> Result<()> {
    let (sub_ca_cert_pem, sub_ca_key_pem) =
        pki::rotate_sub_ca(certs_dir).map_err(|e| anyhow::anyhow!("generating new sub-CA: {e}"))?;

    println!("New sub-CA generated locally:");
    println!("  cert: {}", certs_dir.join("sub-ca.crt").display());
    println!("  key:  {}", certs_dir.join("sub-ca.key").display());

    let mut ctrl = client::controller_client(info).await?;
    let resp = ctrl
        .rotate_sub_ca(client::controller_proto::RotateSubCaRequest {
            sub_ca_cert_pem,
            sub_ca_key_pem,
        })
        .await?
        .into_inner();

    if resp.success {
        println!("Sub-CA pushed to controller: {}", resp.message);
    } else {
        anyhow::bail!("Controller rejected sub-CA rotation: {}", resp.message);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rotate_creates_new_controller_cert() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let certs_dir = tmp.path().join("certs");
        pki::create_cluster_pki(&certs_dir, "10.0.0.1", false).expect("create pki");

        let original_cert =
            std::fs::read_to_string(certs_dir.join("controller.crt")).expect("read cert");

        rotate(&certs_dir, "10.0.0.2:9090", None)
            .await
            .expect("rotate");

        let new_cert =
            std::fs::read_to_string(certs_dir.join("controller.crt")).expect("read new cert");
        assert_ne!(
            original_cert, new_cert,
            "cert should have changed after rotation"
        );

        let ca = std::fs::read_to_string(certs_dir.join("ca.crt")).expect("read ca");
        assert!(!ca.is_empty(), "CA cert should be unchanged");
    }

    #[tokio::test]
    async fn rotate_fails_without_ca() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let certs_dir = tmp.path().join("empty-certs");
        std::fs::create_dir_all(&certs_dir).expect("mkdir");

        let result = rotate(&certs_dir, "10.0.0.1:9090", None).await;
        assert!(result.is_err());
    }
}
