use std::net::IpAddr;
use std::path::{Path, PathBuf};

use rcgen::{BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair};

#[derive(Debug, Clone)]
pub struct ClusterPkiPaths {
    pub certs_dir: PathBuf,
    pub ca_cert: PathBuf,
    pub ca_key: PathBuf,
    pub controller_cert: PathBuf,
    pub controller_key: PathBuf,
    pub kctl_cert: PathBuf,
    pub kctl_key: PathBuf,
}

pub struct InstallPkiPayload {
    pub ca_cert_pem: String,
    pub node_cert_pem: String,
    pub node_key_pem: String,
    pub controller_cert_pem: String,
    pub controller_key_pem: String,
    pub kctl_cert_pem: String,
    pub kctl_key_pem: String,
}

pub fn host_from_address(addr: &str) -> Result<String, String> {
    if addr.starts_with('[') {
        let end = addr
            .find(']')
            .ok_or_else(|| format!("invalid bracketed address: {addr}"))?;
        return Ok(addr[1..end].to_string());
    }

    if let Ok(ip) = addr.parse::<IpAddr>() {
        return Ok(ip.to_string());
    }

    if let Some((host, _port)) = addr.rsplit_once(':') {
        if !host.is_empty() {
            return Ok(host.to_string());
        }
    }

    if !addr.is_empty() {
        return Ok(addr.to_string());
    }

    Err("empty address".to_string())
}

fn sign_cert(
    host: Option<&str>,
    common_name: &str,
    usages: Vec<ExtendedKeyUsagePurpose>,
    ca_cert_pem: &str,
    ca_key_pem: &str,
) -> Result<(String, String), String> {
    let mut params = if let Some(host) = host {
        CertificateParams::new(vec![host.to_string()])
    } else {
        CertificateParams::default()
    };
    params
        .distinguished_name
        .push(DnType::CommonName, common_name.to_string());
    params.extended_key_usages = usages;

    let ca_key = KeyPair::from_pem(ca_key_pem).map_err(|e| format!("loading CA key: {e}"))?;
    let ca_params = CertificateParams::from_ca_cert_pem(ca_cert_pem, ca_key)
        .map_err(|e| format!("loading CA cert: {e}"))?;
    let ca_cert = Certificate::from_params(ca_params).map_err(|e| format!("building CA cert: {e}"))?;

    let cert = Certificate::from_params(params).map_err(|e| format!("building cert: {e}"))?;
    let cert_pem = cert
        .serialize_pem_with_signer(&ca_cert)
        .map_err(|e| format!("signing cert: {e}"))?;
    let key_pem = cert.serialize_private_key_pem();
    Ok((cert_pem, key_pem))
}

fn write_file(path: &Path, content: &str, mode: u32) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }

    std::fs::write(path, content).map_err(|e| format!("writing {}: {e}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|e| format!("chmod {}: {e}", path.display()))?;
    }

    Ok(())
}

pub fn create_cluster_pki(
    certs_dir: &Path,
    controller_host: &str,
    force: bool,
) -> Result<ClusterPkiPaths, String> {
    std::fs::create_dir_all(certs_dir)
        .map_err(|e| format!("creating cert dir {}: {e}", certs_dir.display()))?;

    let paths = ClusterPkiPaths {
        certs_dir: certs_dir.to_path_buf(),
        ca_cert: certs_dir.join("ca.crt"),
        ca_key: certs_dir.join("ca.key"),
        controller_cert: certs_dir.join("controller.crt"),
        controller_key: certs_dir.join("controller.key"),
        kctl_cert: certs_dir.join("kctl.crt"),
        kctl_key: certs_dir.join("kctl.key"),
    };

    if !force
        && [
            &paths.ca_cert,
            &paths.ca_key,
            &paths.controller_cert,
            &paths.controller_key,
            &paths.kctl_cert,
            &paths.kctl_key,
        ]
        .iter()
        .any(|p| p.exists())
    {
        return Err(format!(
            "certificates already exist in {} (use --force to overwrite)",
            certs_dir.display()
        ));
    }

    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "kcore-cluster-ca");
    let ca_cert = Certificate::from_params(ca_params).map_err(|e| format!("CA build: {e}"))?;
    let ca_cert_pem = ca_cert
        .serialize_pem()
        .map_err(|e| format!("CA serialize cert: {e}"))?;
    let ca_key_pem = ca_cert.serialize_private_key_pem();

    let (controller_cert_pem, controller_key_pem) = sign_cert(
        Some(controller_host),
        "kcore-controller",
        vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ],
        &ca_cert_pem,
        &ca_key_pem,
    )?;

    let (kctl_cert_pem, kctl_key_pem) = sign_cert(
        None,
        "kcore-kctl",
        vec![ExtendedKeyUsagePurpose::ClientAuth],
        &ca_cert_pem,
        &ca_key_pem,
    )?;

    write_file(&paths.ca_cert, &ca_cert_pem, 0o644)?;
    write_file(&paths.ca_key, &ca_key_pem, 0o600)?;
    write_file(&paths.controller_cert, &controller_cert_pem, 0o644)?;
    write_file(&paths.controller_key, &controller_key_pem, 0o600)?;
    write_file(&paths.kctl_cert, &kctl_cert_pem, 0o644)?;
    write_file(&paths.kctl_key, &kctl_key_pem, 0o600)?;

    Ok(paths)
}

pub fn load_install_pki(certs_dir: &Path, node_host: &str) -> Result<InstallPkiPayload, String> {
    let ca_cert_path = certs_dir.join("ca.crt");
    let ca_key_path = certs_dir.join("ca.key");
    let controller_cert_path = certs_dir.join("controller.crt");
    let controller_key_path = certs_dir.join("controller.key");
    let kctl_cert_path = certs_dir.join("kctl.crt");
    let kctl_key_path = certs_dir.join("kctl.key");

    let ca_cert_pem =
        std::fs::read_to_string(&ca_cert_path).map_err(|e| format!("reading {}: {e}", ca_cert_path.display()))?;
    let ca_key_pem =
        std::fs::read_to_string(&ca_key_path).map_err(|e| format!("reading {}: {e}", ca_key_path.display()))?;
    let controller_cert_pem = std::fs::read_to_string(&controller_cert_path)
        .map_err(|e| format!("reading {}: {e}", controller_cert_path.display()))?;
    let controller_key_pem = std::fs::read_to_string(&controller_key_path)
        .map_err(|e| format!("reading {}: {e}", controller_key_path.display()))?;
    let kctl_cert_pem =
        std::fs::read_to_string(&kctl_cert_path).map_err(|e| format!("reading {}: {e}", kctl_cert_path.display()))?;
    let kctl_key_pem =
        std::fs::read_to_string(&kctl_key_path).map_err(|e| format!("reading {}: {e}", kctl_key_path.display()))?;

    let (node_cert_pem, node_key_pem) = sign_cert(
        Some(node_host),
        &format!("kcore-node-{node_host}"),
        vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ],
        &ca_cert_pem,
        &ca_key_pem,
    )?;

    Ok(InstallPkiPayload {
        ca_cert_pem,
        node_cert_pem,
        node_key_pem,
        controller_cert_pem,
        controller_key_pem,
        kctl_cert_pem,
        kctl_key_pem,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_host_from_host_port() {
        let host = host_from_address("10.0.0.12:9091").expect("host parse");
        assert_eq!(host, "10.0.0.12");
    }

    #[test]
    fn creates_cluster_pki_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let certs = tmp.path().join("certs");

        let out = create_cluster_pki(&certs, "127.0.0.1", false).expect("create pki");

        assert!(out.ca_cert.exists());
        assert!(out.ca_key.exists());
        assert!(out.controller_cert.exists());
        assert!(out.controller_key.exists());
        assert!(out.kctl_cert.exists());
        assert!(out.kctl_key.exists());
    }
}
