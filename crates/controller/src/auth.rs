use tonic::transport::server::{TcpConnectInfo, TlsConnectInfo};
use tonic::{Request, Status};

pub const CN_KCTL: &str = "kcore-kctl";
pub const CN_NODE_PREFIX: &str = "kcore-node-";
pub const CN_CONTROLLER_PREFIX: &str = "kcore-controller-";

/// Extract the Common Name from the peer's TLS client certificate.
/// Returns `None` when TLS is not in use or no client cert was presented.
pub fn peer_cn<T>(request: &Request<T>) -> Option<String> {
    let tls_info = request
        .extensions()
        .get::<TlsConnectInfo<TcpConnectInfo>>()?;
    let certs = tls_info.peer_certs()?;
    let cert_der = certs.first()?;

    use x509_parser::prelude::FromDer;
    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(cert_der.as_ref()).ok()?;
    let cn = cert
        .subject()
        .iter_common_name()
        .next()?
        .as_str()
        .ok()
        .map(String::from);
    cn
}

/// Require that the peer's certificate CN matches one of the allowed patterns.
///
/// Patterns ending with `-` are treated as prefixes (for node certs like
/// `kcore-node-10.0.0.1`). All other patterns require an exact match.
///
/// When TLS is not in use (insecure mode), authorization is skipped — the
/// startup-time `--allow-insecure` enforcement is the primary control.
#[allow(clippy::result_large_err)]
pub fn require_peer<T>(request: &Request<T>, allowed: &[&str]) -> Result<(), Status> {
    let cn = match peer_cn(request) {
        Some(cn) => cn,
        None => return Ok(()),
    };

    if is_authorized(&cn, allowed) {
        Ok(())
    } else {
        Err(Status::permission_denied(format!(
            "peer '{cn}' is not authorized for this operation"
        )))
    }
}

fn is_authorized(cn: &str, allowed: &[&str]) -> bool {
    allowed.iter().any(|pattern| {
        if pattern.ends_with('-') {
            cn.starts_with(pattern)
        } else {
            cn == *pattern
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_prefix_matching() {
        assert!(is_authorized("kcore-node-10.0.0.1", &[CN_NODE_PREFIX]));
        assert!(is_authorized("kcore-node-192.168.1.1", &[CN_NODE_PREFIX]));
        assert!(!is_authorized("kcore-controller", &[CN_NODE_PREFIX]));
        assert!(!is_authorized("kcore-kctl", &[CN_NODE_PREFIX]));
    }

    #[test]
    fn exact_matching() {
        assert!(is_authorized("kcore-kctl", &[CN_KCTL]));
        assert!(!is_authorized("kcore-kctl-evil", &[CN_KCTL]));
        assert!(!is_authorized("kcore-controller", &[CN_KCTL]));
    }

    #[test]
    fn multiple_allowed_patterns() {
        let allowed = &[CN_KCTL, CN_NODE_PREFIX];
        assert!(is_authorized("kcore-kctl", allowed));
        assert!(is_authorized("kcore-node-10.0.0.1", allowed));
        assert!(!is_authorized("kcore-controller", allowed));
    }

    #[test]
    fn require_peer_allows_missing_tls_info() {
        let request = Request::new(());
        assert!(require_peer(&request, &[CN_KCTL]).is_ok());
    }
}
