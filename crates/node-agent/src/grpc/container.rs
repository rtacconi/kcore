use tonic::{Request, Response, Status};

use crate::auth::{self, CN_CONTROLLER_PREFIX, CN_KCTL};
use crate::proto;
use crate::runtime::cni;
use crate::runtime::containerd_runtime::ContainerdRuntime;

pub struct ContainerService;

impl ContainerService {
    pub fn new() -> Self {
        Self
    }
}

fn state_from_status(status: &str) -> i32 {
    let normalized = status.to_ascii_lowercase();
    if normalized.starts_with("up ") || normalized == "running" {
        proto::ContainerState::Running as i32
    } else if normalized.starts_with("exited") || normalized.starts_with("created") {
        proto::ContainerState::Stopped as i32
    } else {
        proto::ContainerState::Unknown as i32
    }
}

fn parse_list_line(line: &str) -> Option<proto::ContainerInfo> {
    let mut parts = line.splitn(4, '\t');
    let id = parts.next()?.trim().to_string();
    let name = parts.next()?.trim().to_string();
    let image = parts.next()?.trim().to_string();
    let status = parts.next()?.trim().to_string();
    if id.is_empty() || name.is_empty() {
        return None;
    }
    Some(proto::ContainerInfo {
        id,
        name,
        image,
        state: state_from_status(&status),
        status,
    })
}

#[allow(clippy::result_large_err)]
fn prepare_storage_mount(
    container_name: &str,
    backend: &str,
    size_bytes: i64,
    mount_target: &str,
) -> Result<Option<(String, String)>, Status> {
    let backend = backend.trim().to_ascii_lowercase();
    if backend.is_empty() {
        return Ok(None);
    }
    let base = match backend.as_str() {
        "filesystem" => "/var/lib/kcore/volumes/filesystem",
        "lvm" => "/var/lib/kcore/volumes/lvm",
        "zfs" => "/var/lib/kcore/volumes/zfs",
        other => {
            return Err(Status::invalid_argument(format!(
                "unsupported storage_backend '{other}', expected filesystem|lvm|zfs"
            )))
        }
    };
    // CRITICAL: `container_name` is forwarded by the controller from the
    // user manifest. Without sanitization a name like `../../etc` would
    // make `create_dir_all` traverse out of the volumes root and create
    // (or reuse!) directories anywhere on the host. Container names are
    // single-segment identifiers, so reject anything with a separator.
    let safe_name = crate::path_safety::validate_safe_segment(container_name, "container name")
        .map_err(Status::invalid_argument)?;
    let host_path = format!("{base}/{safe_name}");

    // Validate the operator-supplied mount target inside the *container*:
    // it must be an absolute path (containerd/runc rejects relative
    // mount targets) and must not contain `\0`.
    let target = if mount_target.trim().is_empty() {
        "/data".to_string()
    } else {
        let t = mount_target.trim();
        if t.contains('\0') {
            return Err(Status::invalid_argument(
                "mount_target must not contain NUL bytes",
            ));
        }
        if !t.starts_with('/') {
            return Err(Status::invalid_argument(
                "mount_target must be an absolute path inside the container",
            ));
        }
        t.to_string()
    };

    std::fs::create_dir_all(&host_path).map_err(|e| {
        Status::internal(format!(
            "creating storage path for backend '{backend}' at {host_path}: {e}"
        ))
    })?;

    if size_bytes > 0 {
        let quota_file = format!("{host_path}/.kcore.requested_size_bytes");
        std::fs::write(&quota_file, size_bytes.to_string()).map_err(|e| {
            Status::internal(format!(
                "writing storage quota metadata at {quota_file}: {e}"
            ))
        })?;
    }

    Ok(Some((host_path, target)))
}

async fn inspect_container(
    rt: &ContainerdRuntime,
    name: &str,
) -> Result<proto::ContainerInfo, Status> {
    let format = "{{.ID}}\t{{.Name}}\t{{.Config.Image}}\t{{.State.Status}}";
    let args = vec![
        "inspect".to_string(),
        "--format".to_string(),
        format.to_string(),
        name.to_string(),
    ];
    let out = rt.run(&args).await?;
    let mut parts = out.splitn(4, '\t');
    let id = parts.next().unwrap_or_default().trim().to_string();
    let raw_name = parts.next().unwrap_or_default().trim().to_string();
    let image = parts.next().unwrap_or_default().trim().to_string();
    let status = parts.next().unwrap_or_default().trim().to_string();
    let name = raw_name.trim_start_matches('/').to_string();
    if id.is_empty() || name.is_empty() {
        return Err(Status::internal(format!(
            "unexpected inspect output for container {name}"
        )));
    }
    Ok(proto::ContainerInfo {
        id,
        name,
        image,
        state: state_from_status(&status),
        status,
    })
}

#[tonic::async_trait]
impl proto::node_container_server::NodeContainer for ContainerService {
    async fn create_container(
        &self,
        request: Request<proto::CreateContainerRequest>,
    ) -> Result<Response<proto::CreateContainerResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;
        let req = request.into_inner();
        let spec = req
            .spec
            .ok_or_else(|| Status::invalid_argument("spec is required"))?;
        let name = spec.name.trim();
        let image = spec.image.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("spec.name is required"));
        }
        if image.is_empty() {
            return Err(Status::invalid_argument("spec.image is required"));
        }
        let rt = ContainerdRuntime::detect().await?;

        let mut args = vec![
            "run".to_string(),
            "-d".to_string(),
            "--name".to_string(),
            name.to_string(),
        ];
        let network = spec.network.trim();
        if !network.is_empty() {
            cni::ensure_bridge_backed_network(network)?;
            args.push("--network".to_string());
            args.push(network.to_string());
        }
        for port in spec.ports {
            let p = port.trim();
            if p.is_empty() {
                continue;
            }
            args.push("-p".to_string());
            args.push(p.to_string());
        }
        for (k, v) in spec.env {
            if k.trim().is_empty() {
                continue;
            }
            args.push("-e".to_string());
            args.push(format!("{k}={v}"));
        }
        if let Some((host, target)) = prepare_storage_mount(
            name,
            &spec.storage_backend,
            spec.storage_size_bytes,
            &spec.mount_target,
        )? {
            args.push("-v".to_string());
            args.push(format!("{host}:{target}"));
        }
        args.push(image.to_string());
        for cmd in spec.command {
            if !cmd.trim().is_empty() {
                args.push(cmd);
            }
        }

        let _id = rt.run(&args).await?;
        let info = inspect_container(&rt, name).await?;
        Ok(Response::new(proto::CreateContainerResponse {
            container: Some(info),
        }))
    }

    async fn start_container(
        &self,
        request: Request<proto::StartContainerRequest>,
    ) -> Result<Response<proto::StartContainerResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;
        let name = request.into_inner().name;
        let name = name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }
        let rt = ContainerdRuntime::detect().await?;
        let args = vec!["start".to_string(), name.to_string()];
        let _ = rt.run(&args).await?;
        let info = inspect_container(&rt, name).await?;
        Ok(Response::new(proto::StartContainerResponse {
            container: Some(info),
        }))
    }

    async fn stop_container(
        &self,
        request: Request<proto::StopContainerRequest>,
    ) -> Result<Response<proto::StopContainerResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;
        let name = request.into_inner().name;
        let name = name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }
        let rt = ContainerdRuntime::detect().await?;
        let args = vec!["stop".to_string(), name.to_string()];
        let _ = rt.run(&args).await?;
        let info = inspect_container(&rt, name).await?;
        Ok(Response::new(proto::StopContainerResponse {
            container: Some(info),
        }))
    }

    async fn delete_container(
        &self,
        request: Request<proto::DeleteContainerRequest>,
    ) -> Result<Response<proto::DeleteContainerResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;
        let req = request.into_inner();
        let name = req.name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }
        let rt = ContainerdRuntime::detect().await?;
        let mut args = vec!["rm".to_string()];
        if req.force {
            args.push("-f".to_string());
        }
        args.push(name.to_string());
        let _ = rt.run(&args).await?;
        Ok(Response::new(proto::DeleteContainerResponse {
            success: true,
        }))
    }

    async fn get_container(
        &self,
        request: Request<proto::GetContainerRequest>,
    ) -> Result<Response<proto::GetContainerResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;
        let name = request.into_inner().name;
        let name = name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }
        let rt = ContainerdRuntime::detect().await?;
        let info = inspect_container(&rt, name).await?;
        Ok(Response::new(proto::GetContainerResponse {
            container: Some(info),
        }))
    }

    async fn list_containers(
        &self,
        request: Request<proto::ListContainersRequest>,
    ) -> Result<Response<proto::ListContainersResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;
        let rt = ContainerdRuntime::detect().await?;
        let args = vec![
            "ps".to_string(),
            "-a".to_string(),
            "--format".to_string(),
            "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}".to_string(),
        ];
        let out = rt.run(&args).await?;
        let containers = out.lines().filter_map(parse_list_line).collect();
        Ok(Response::new(proto::ListContainersResponse { containers }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_state_mapping() {
        assert_eq!(
            state_from_status("Up 2 minutes"),
            proto::ContainerState::Running as i32
        );
        assert_eq!(
            state_from_status("running"),
            proto::ContainerState::Running as i32
        );
        assert_eq!(
            state_from_status("Exited (0) 10s ago"),
            proto::ContainerState::Stopped as i32
        );
    }

    #[test]
    fn parses_container_line() {
        let line = "abc123\tnginx-demo\tnginx:alpine\tUp 5 seconds";
        let parsed = parse_list_line(line).expect("parsed");
        assert_eq!(parsed.id, "abc123");
        assert_eq!(parsed.name, "nginx-demo");
        assert_eq!(parsed.image, "nginx:alpine");
        assert_eq!(parsed.state, proto::ContainerState::Running as i32);
    }

    #[test]
    fn prepare_storage_mount_rejects_path_traversal_in_container_name() {
        // Regression: an attacker-controlled `container_name` like `../etc`
        // used to be joined under /var/lib/kcore/volumes/<backend>/ as a
        // raw string, so `create_dir_all` happily traversed out of the
        // volumes root.
        let err = prepare_storage_mount("../etc", "filesystem", 0, "")
            .expect_err("must reject traversal");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        let err = prepare_storage_mount("foo/bar", "filesystem", 0, "")
            .expect_err("must reject path separator");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        let err = prepare_storage_mount("", "filesystem", 0, "").expect_err("must reject empty");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        let err =
            prepare_storage_mount("foo\0bar", "filesystem", 0, "").expect_err("must reject NUL");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn prepare_storage_mount_rejects_relative_mount_target() {
        let err = prepare_storage_mount("web", "filesystem", 0, "data")
            .expect_err("relative mount target must be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn prepare_storage_mount_returns_none_when_backend_empty() {
        // Empty backend short-circuits before any filesystem work, so this
        // is safe to run without a writable host root.
        assert!(prepare_storage_mount("web", "", 0, "").unwrap().is_none());
    }

    #[test]
    fn prepare_storage_mount_rejects_unknown_backend() {
        let err = prepare_storage_mount("web", "btrfs", 0, "")
            .expect_err("unknown backend must be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }
}
