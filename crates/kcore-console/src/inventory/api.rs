//! Local kcore API reachability (TCP probe to the node-agent port).

use std::net::TcpStream;
use std::time::Duration;

/// Default node-agent gRPC port (kcore).
pub const KCORE_API_PORT: u16 = 9091;
const DIAL: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiStatus {
    Unavailable,
    /// Port accepted a connection; `healthy` if TCP probe is enough for now.
    Reachable {
        healthy: bool,
    },
}

/// TCP connect to 127.0.0.1:9091 (or `KCORE_API_PORT`).
/// If nothing listens, [ApiStatus::Unavailable].
pub fn probe() -> ApiStatus {
    let port: u16 = std::env::var("KCORE_API_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(KCORE_API_PORT);
    if matches_peer(port) {
        ApiStatus::Reachable { healthy: true }
    } else {
        ApiStatus::Unavailable
    }
}

fn matches_peer(port: u16) -> bool {
    let addr: std::net::SocketAddr = (std::net::Ipv4Addr::LOCALHOST, port).into();
    TcpStream::connect_timeout(&addr, DIAL).is_ok()
}

impl std::fmt::Display for ApiStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiStatus::Unavailable => write!(f, "unavailable"),
            ApiStatus::Reachable { healthy: true } => write!(f, "available"),
            ApiStatus::Reachable { healthy: false } => write!(f, "degraded"),
        }
    }
}
