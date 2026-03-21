use std::path::{Path, PathBuf};

use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::client::legacy::Client as HyperClient;
use hyperlocal::{UnixClientExt, Uri};
use tracing::warn;

use super::types::VmInfo;

/// Read-only client that discovers Cloud Hypervisor API sockets in a directory
/// and queries them for VM status.
#[derive(Clone)]
pub struct Client {
    socket_dir: PathBuf,
}

impl Client {
    pub fn new(socket_dir: &str) -> Self {
        Self {
            socket_dir: PathBuf::from(socket_dir),
        }
    }

    /// List all VM names by scanning for `*.sock` files in the socket directory.
    pub fn list_vm_names(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.socket_dir) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) == Some("sock") {
                    path.file_stem().and_then(|s| s.to_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Query a single VM's status via its Cloud Hypervisor API socket.
    pub async fn get_vm_info(&self, name: &str) -> Option<VmInfo> {
        let socket_path = self.socket_dir.join(format!("{name}.sock"));
        query_vm_info(&socket_path).await
    }

    /// Query all VMs and return (name, info) pairs.
    pub async fn list_vms(&self) -> Vec<(String, VmInfo)> {
        let names = self.list_vm_names();
        let mut results = Vec::with_capacity(names.len());
        for name in names {
            if let Some(info) = self.get_vm_info(&name).await {
                results.push((name, info));
            }
        }
        results
    }
}

async fn query_vm_info(socket_path: &Path) -> Option<VmInfo> {
    let client = HyperClient::unix();
    let uri = Uri::new(socket_path, "/api/v1/vm.info");

    let req = Request::get(uri).body(Empty::<Bytes>::new()).ok()?;

    let resp = match client.request(req).await {
        Ok(r) => r,
        Err(e) => {
            warn!(socket = %socket_path.display(), error = %e, "failed to query CH socket");
            return None;
        }
    };

    let body = resp.into_body().collect().await.ok()?.to_bytes();
    serde_json::from_slice(&body).ok()
}
