use rcgen::{CertificateParams, DnType, ExtendedKeyUsagePurpose, Issuer, KeyPair};
use time::{Duration, OffsetDateTime};

const CERT_VALIDITY_DAYS: i64 = 365;

/// Sign a new node certificate using the sub-CA, returning (chain_pem, key_pem).
/// chain_pem = leaf cert + sub-CA cert concatenated.
pub fn sign_node_cert(
    sub_ca_cert_pem: &str,
    sub_ca_key_pem: &str,
    node_host: &str,
) -> Result<(String, String), String> {
    let mut params = CertificateParams::new(vec![node_host.to_string()])
        .map_err(|e| format!("invalid SAN: {e}"))?;
    params
        .distinguished_name
        .push(DnType::CommonName, format!("kcore-node-{node_host}"));
    params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ServerAuth,
        ExtendedKeyUsagePurpose::ClientAuth,
    ];
    params.not_before = OffsetDateTime::now_utc();
    params.not_after = OffsetDateTime::now_utc() + Duration::days(CERT_VALIDITY_DAYS);

    let ca_key =
        KeyPair::from_pem(sub_ca_key_pem).map_err(|e| format!("loading sub-CA key: {e}"))?;
    let issuer = Issuer::from_ca_cert_pem(sub_ca_cert_pem, ca_key)
        .map_err(|e| format!("loading sub-CA cert: {e}"))?;

    let cert_key = KeyPair::generate().map_err(|e| format!("generating node key: {e}"))?;
    let cert = params
        .signed_by(&cert_key, &issuer)
        .map_err(|e| format!("signing node cert: {e}"))?;

    let chain_pem = format!("{}{}", cert.pem(), sub_ca_cert_pem);
    Ok((chain_pem, cert_key.serialize_pem()))
}

/// Validate that a PEM string is a parseable X.509 certificate with CA
/// basicConstraints.
pub fn validate_sub_ca_cert(cert_pem: &str) -> Result<(), String> {
    let pem = pem::parse(cert_pem).map_err(|e| format!("PEM parse error: {e}"))?;
    use x509_parser::prelude::FromDer;
    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(pem.contents())
        .map_err(|e| format!("X.509 parse error: {e}"))?;
    let bc = cert
        .basic_constraints()
        .map_err(|e| format!("reading basicConstraints: {e}"))?
        .ok_or("certificate has no basicConstraints extension")?;
    if !bc.value.ca {
        return Err("certificate is not a CA".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_test_ca() -> (String, String) {
        use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa};
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(DnType::CommonName, "test-ca");
        params.not_before = OffsetDateTime::now_utc();
        params.not_after = OffsetDateTime::now_utc() + Duration::days(3650);
        let key = KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        (cert.pem(), key.serialize_pem())
    }

    fn generate_test_sub_ca(ca_cert_pem: &str, ca_key_pem: &str) -> (String, String) {
        use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair};
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params
            .distinguished_name
            .push(DnType::CommonName, "test-sub-ca");
        params.not_before = OffsetDateTime::now_utc();
        params.not_after = OffsetDateTime::now_utc() + Duration::days(1825);
        let ca_key = KeyPair::from_pem(ca_key_pem).unwrap();
        let issuer = Issuer::from_ca_cert_pem(ca_cert_pem, ca_key).unwrap();
        let sub_key = KeyPair::generate().unwrap();
        let sub_cert = params.signed_by(&sub_key, &issuer).unwrap();
        (sub_cert.pem(), sub_key.serialize_pem())
    }

    #[test]
    fn sign_node_cert_produces_chain() {
        let (ca_cert, ca_key) = generate_test_ca();
        let (sub_cert, sub_key) = generate_test_sub_ca(&ca_cert, &ca_key);
        let (chain, key) = sign_node_cert(&sub_cert, &sub_key, "10.0.0.50").unwrap();
        assert!(key.contains("BEGIN PRIVATE KEY"));
        assert_eq!(chain.matches("BEGIN CERTIFICATE").count(), 2);
    }

    #[test]
    fn validate_sub_ca_cert_accepts_ca() {
        let (ca_cert, ca_key) = generate_test_ca();
        let (sub_cert, _) = generate_test_sub_ca(&ca_cert, &ca_key);
        validate_sub_ca_cert(&sub_cert).unwrap();
    }

    #[test]
    fn validate_sub_ca_cert_rejects_leaf() {
        let (ca_cert, ca_key) = generate_test_ca();
        let (leaf, _) = sign_node_cert(
            &{
                let (sc, sk) = generate_test_sub_ca(&ca_cert, &ca_key);
                let _ = sk;
                sc
            },
            &generate_test_sub_ca(&ca_cert, &ca_key).1,
            "10.0.0.1",
        )
        .unwrap();
        let first_cert = leaf
            .split("-----END CERTIFICATE-----")
            .next()
            .unwrap()
            .to_string()
            + "-----END CERTIFICATE-----\n";
        let err = validate_sub_ca_cert(&first_cert);
        assert!(err.is_err());
    }
}
