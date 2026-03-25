use std::net::IpAddr;
use std::path::{Path, PathBuf};

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair,
};
use time::{Duration, OffsetDateTime};

const CA_VALIDITY_DAYS: i64 = 3650; // ~10 years
const CERT_VALIDITY_DAYS: i64 = 365; // 1 year

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
    /// Only populated when the node will also run the controller.
    pub controller_cert_pem: String,
    /// Only populated when the node will also run the controller.
    pub controller_key_pem: String,
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
    params.not_before = OffsetDateTime::now_utc();
    params.not_after = OffsetDateTime::now_utc() + Duration::days(CERT_VALIDITY_DAYS);

    let ca_key = KeyPair::from_pem(ca_key_pem).map_err(|e| format!("loading CA key: {e}"))?;
    let ca_params = CertificateParams::from_ca_cert_pem(ca_cert_pem, ca_key)
        .map_err(|e| format!("loading CA cert: {e}"))?;
    let ca_cert =
        Certificate::from_params(ca_params).map_err(|e| format!("building CA cert: {e}"))?;

    let cert = Certificate::from_params(params).map_err(|e| format!("building cert: {e}"))?;
    let cert_pem = cert
        .serialize_pem_with_signer(&ca_cert)
        .map_err(|e| format!("signing cert: {e}"))?;
    let key_pem = cert.serialize_private_key_pem();
    Ok((cert_pem, key_pem))
}

fn write_file(path: &Path, content: &str, mode: u32) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creating {}: {e}", parent.display()))?;
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
    ca_params.not_before = OffsetDateTime::now_utc();
    ca_params.not_after = OffsetDateTime::now_utc() + Duration::days(CA_VALIDITY_DAYS);
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

/// Load PKI material for a node install.
///
/// The CA key is read locally to sign the node certificate but is never sent
/// over the wire. Controller cert/key are only included when
/// `include_controller_pki` is true (the node will co-locate the controller).
/// kctl credentials are never sent -- nodes have no use for CLI keys.
pub fn load_install_pki(
    certs_dir: &Path,
    node_host: &str,
    include_controller_pki: bool,
) -> Result<InstallPkiPayload, String> {
    let ca_cert_path = certs_dir.join("ca.crt");
    let ca_key_path = certs_dir.join("ca.key");

    let mut required: Vec<&Path> = vec![&ca_cert_path, &ca_key_path];

    let controller_cert_path = certs_dir.join("controller.crt");
    let controller_key_path = certs_dir.join("controller.key");
    if include_controller_pki {
        required.push(&controller_cert_path);
        required.push(&controller_key_path);
    }

    let missing: Vec<String> = required
        .iter()
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "missing install bootstrap PKI files in {}: {}. \
run `kctl create cluster --context <cluster-name> --controller <host:9090>` and select that context before node install",
            certs_dir.display(),
            missing.join(", ")
        ));
    }

    let ca_cert_pem = std::fs::read_to_string(&ca_cert_path)
        .map_err(|e| format!("reading {}: {e}", ca_cert_path.display()))?;
    let ca_key_pem = std::fs::read_to_string(&ca_key_path)
        .map_err(|e| format!("reading {}: {e}", ca_key_path.display()))?;

    let (controller_cert_pem, controller_key_pem) = if include_controller_pki {
        let cert = std::fs::read_to_string(&controller_cert_path)
            .map_err(|e| format!("reading {}: {e}", controller_cert_path.display()))?;
        let key = std::fs::read_to_string(&controller_key_path)
            .map_err(|e| format!("reading {}: {e}", controller_key_path.display()))?;
        (cert, key)
    } else {
        (String::new(), String::new())
    };

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

    #[test]
    fn load_install_pki_without_controller_omits_controller_keys() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let certs = tmp.path().join("certs");
        create_cluster_pki(&certs, "127.0.0.1", false).expect("create pki");

        let payload =
            load_install_pki(&certs, "10.0.0.21", false).expect("load install pki");

        assert!(!payload.ca_cert_pem.is_empty());
        assert!(!payload.node_cert_pem.is_empty());
        assert!(!payload.node_key_pem.is_empty());
        assert!(
            payload.controller_cert_pem.is_empty(),
            "controller cert should not be sent to worker nodes"
        );
        assert!(
            payload.controller_key_pem.is_empty(),
            "controller key should not be sent to worker nodes"
        );
    }

    #[test]
    fn load_install_pki_with_controller_includes_controller_keys() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let certs = tmp.path().join("certs");
        create_cluster_pki(&certs, "127.0.0.1", false).expect("create pki");

        let payload =
            load_install_pki(&certs, "127.0.0.1", true).expect("load install pki");

        assert!(!payload.ca_cert_pem.is_empty());
        assert!(!payload.node_cert_pem.is_empty());
        assert!(!payload.node_key_pem.is_empty());
        assert!(
            !payload.controller_cert_pem.is_empty(),
            "controller cert should be sent when node co-locates controller"
        );
        assert!(
            !payload.controller_key_pem.is_empty(),
            "controller key should be sent when node co-locates controller"
        );
    }
}
