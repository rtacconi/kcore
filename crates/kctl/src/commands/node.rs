use crate::client::{self, controller_proto, node_proto};
use crate::config::ConnectionInfo;
use crate::output;
use crate::pki;
use anyhow::{Context, Result};
use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio_stream::wrappers::ReceiverStream;

pub async fn approve(info: &ConnectionInfo, node_id: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .approve_node(controller_proto::ApproveNodeRequest {
            node_id: node_id.to_string(),
        })
        .await?
        .into_inner();

    if resp.success {
        println!("{}", resp.message);
        Ok(())
    } else {
        anyhow::bail!("approve failed: {}", resp.message)
    }
}

pub async fn reject(info: &ConnectionInfo, node_id: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .reject_node(controller_proto::RejectNodeRequest {
            node_id: node_id.to_string(),
        })
        .await?
        .into_inner();

    if resp.success {
        println!("{}", resp.message);
        Ok(())
    } else {
        anyhow::bail!("reject failed: {}", resp.message)
    }
}

pub async fn list_nodes(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .list_nodes(controller_proto::ListNodesRequest {})
        .await?
        .into_inner();

    if resp.nodes.is_empty() {
        println!("No nodes found");
        return Ok(());
    }

    output::print_node_table(&resp.nodes);
    Ok(())
}

pub async fn get_node(info: &ConnectionInfo, node_id: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .get_node(controller_proto::GetNodeRequest {
            node_id: node_id.to_string(),
        })
        .await?
        .into_inner();

    let node = resp.node.as_ref().context("node not found")?;
    output::print_node_detail(node);

    let vms_resp = client
        .list_vms(controller_proto::ListVmsRequest {
            target_node: node_id.to_string(),
        })
        .await?
        .into_inner();

    if !vms_resp.vms.is_empty() {
        println!("\nVMs on this node ({}):", vms_resp.vms.len());
        for vm in &vms_resp.vms {
            let state = match vm.state {
                1 => "Stopped",
                2 => "Running",
                3 => "Paused",
                4 => "Error",
                _ => "Unknown",
            };
            println!("  {:<36}  {:<20}  {:<10}", vm.id, vm.name, state);
        }
    }
    Ok(())
}

pub async fn disks(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .list_disks(node_proto::ListDisksRequest {})
        .await?
        .into_inner();

    if resp.disks.is_empty() {
        println!("No disks found");
        return Ok(());
    }

    output::print_disk_table(&resp.disks);
    Ok(())
}

pub async fn nics(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .list_network_interfaces(node_proto::ListNetworkInterfacesRequest {})
        .await?
        .into_inner();

    if resp.interfaces.is_empty() {
        println!("No network interfaces found");
        return Ok(());
    }

    output::print_nic_table(&resp.interfaces);
    Ok(())
}

pub async fn install(
    info: &ConnectionInfo,
    os_disk: &str,
    data_disks: Vec<String>,
    join_controllers: &[String],
    run_controller: bool,
    data_disk_mode: &str,
    storage_backend: Option<&str>,
    lvm_vg_name: Option<&str>,
    lvm_lv_prefix: Option<&str>,
    zfs_pool_name: Option<&str>,
    zfs_dataset_prefix: Option<&str>,
    certs_dir: &Path,
    disable_vxlan: bool,
    dc_id: &str,
    hostname: Option<&str>,
    node_id: Option<&str>,
) -> Result<()> {
    let join_controllers = validate_install_controller_mode(join_controllers, run_controller)?;
    let primary_controller = join_controllers
        .first()
        .cloned()
        .unwrap_or_default();

    let node_host =
        pki::host_from_address(&info.address).map_err(|e| anyhow::anyhow!("node address: {e}"))?;

    let node_is_controller = run_controller;

    let install_pki = pki::load_install_pki(certs_dir, &node_host, node_is_controller)
        .map_err(|e| anyhow::anyhow!("loading PKI: {e}"))?;

    let typed_storage_backend = storage_backend
        .map(storage_backend_to_proto)
        .unwrap_or(node_proto::StorageBackendType::Unspecified as i32);
    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .install_to_disk(node_proto::InstallToDiskRequest {
            os_disk: os_disk.to_string(),
            data_disks,
            controller: primary_controller,
            controllers: join_controllers,
            run_controller,
            ca_cert_pem: install_pki.ca_cert_pem,
            node_cert_pem: install_pki.node_cert_pem,
            node_key_pem: install_pki.node_key_pem,
            controller_cert_pem: install_pki.controller_cert_pem,
            controller_key_pem: install_pki.controller_key_pem,
            kctl_cert_pem: String::new(),
            kctl_key_pem: String::new(),
            sub_ca_cert_pem: install_pki.sub_ca_cert_pem,
            sub_ca_key_pem: install_pki.sub_ca_key_pem,
            data_disk_mode: data_disk_mode.trim().to_string(),
            storage_backend: typed_storage_backend,
            lvm_vg_name: lvm_vg_name.unwrap_or("").trim().to_string(),
            lvm_lv_prefix: lvm_lv_prefix.unwrap_or("").trim().to_string(),
            zfs_pool_name: zfs_pool_name.unwrap_or("").trim().to_string(),
            zfs_dataset_prefix: zfs_dataset_prefix.unwrap_or("").trim().to_string(),
            disable_vxlan,
            dc_id: dc_id.trim().to_string(),
            hostname: hostname.unwrap_or("").trim().to_string(),
            node_id: node_id.unwrap_or("").trim().to_string(),
        })
        .await?
        .into_inner();

    if resp.accepted {
        println!("Install accepted: {}", resp.message);
        Ok(())
    } else {
        anyhow::bail!("Install rejected: {}", resp.message);
    }
}

fn validate_install_controller_mode(
    join_controllers: &[String],
    run_controller: bool,
) -> Result<Vec<String>> {
    let normalized: Vec<String> = join_controllers
        .iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    let has_join = !normalized.is_empty();
    if has_join == run_controller {
        anyhow::bail!("provide exactly one of --join-controller <host:port> or --run-controller");
    }
    Ok(normalized)
}

fn storage_backend_to_proto(value: &str) -> i32 {
    match value.trim().to_ascii_lowercase().as_str() {
        "filesystem" => node_proto::StorageBackendType::Filesystem as i32,
        "lvm" => node_proto::StorageBackendType::Lvm as i32,
        "zfs" => node_proto::StorageBackendType::Zfs as i32,
        _ => node_proto::StorageBackendType::Unspecified as i32,
    }
}

pub async fn apply_nix(info: &ConnectionInfo, file: &str, rebuild: bool) -> Result<()> {
    let content = std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?;

    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .apply_nix_config(node_proto::ApplyNixConfigRequest {
            configuration_nix: content,
            rebuild,
        })
        .await?
        .into_inner();

    if resp.success {
        println!("{}", resp.message);
        Ok(())
    } else {
        anyhow::bail!("Apply failed: {}", resp.message);
    }
}

pub async fn upload_image(
    info: &ConnectionInfo,
    file: &str,
    destination_name: Option<&str>,
    format: Option<&str>,
    image_sha256: Option<&str>,
) -> Result<()> {
    let mut f = tokio::fs::File::open(file)
        .await
        .with_context(|| format!("opening {file}"))?;
    let source_name = Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();
    let detected = format
        .map(|f| f.to_string())
        .unwrap_or_else(|| infer_image_format_from_name(&source_name));
    if detected != "raw" && detected != "qcow2" {
        anyhow::bail!("image format must be raw or qcow2");
    }

    const CHUNK_SIZE: usize = 4 * 1024 * 1024;
    let (tx, rx) = tokio::sync::mpsc::channel::<node_proto::UploadImageChunk>(8);
    let source_name_clone = source_name.clone();
    let destination_name_clone = destination_name.unwrap_or("").to_string();
    let image_sha_clone = image_sha256.unwrap_or("").to_string();
    let detected_clone = detected.clone();
    tokio::spawn(async move {
        let mut first = true;
        loop {
            let mut buf = vec![0u8; CHUNK_SIZE];
            let read = match f.read(&mut buf).await {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("failed to read upload file chunk: {e}");
                    return;
                }
            };
            if read == 0 {
                if first {
                    let _ = tx
                        .send(node_proto::UploadImageChunk {
                            chunk_data: Vec::new(),
                            source_name: source_name_clone.clone(),
                            destination_name: destination_name_clone.clone(),
                            image_format: detected_clone.clone(),
                            image_sha256: image_sha_clone.clone(),
                        })
                        .await;
                }
                return;
            }
            buf.truncate(read);
            let msg = node_proto::UploadImageChunk {
                chunk_data: buf,
                source_name: if first {
                    source_name_clone.clone()
                } else {
                    String::new()
                },
                destination_name: if first {
                    destination_name_clone.clone()
                } else {
                    String::new()
                },
                image_format: if first {
                    detected_clone.clone()
                } else {
                    String::new()
                },
                image_sha256: if first {
                    image_sha_clone.clone()
                } else {
                    String::new()
                },
            };
            first = false;
            if tx.send(msg).await.is_err() {
                return;
            }
        }
    });

    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .upload_image_stream(tonic::Request::new(ReceiverStream::new(rx)))
        .await?
        .into_inner();
    println!("Uploaded image to {}", resp.path);
    println!("  Size:   {}", client::format_bytes(resp.size_bytes));
    println!("  Format: {}", resp.image_format);
    println!("  SHA256: {}", resp.image_sha256);
    Ok(())
}

pub async fn check_vm_ssh_ready(
    info: &ConnectionInfo,
    vm_name: &str,
    network: Option<&str>,
    port: i32,
    timeout_ms: i32,
) -> Result<node_proto::CheckVmSshReadyResponse> {
    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .check_vm_ssh_ready(node_proto::CheckVmSshReadyRequest {
            vm_name: vm_name.to_string(),
            network: network.unwrap_or("").to_string(),
            port,
            timeout_ms,
        })
        .await?
        .into_inner();
    Ok(resp)
}

fn infer_image_format_from_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".qcow2") || lower.ends_with(".qcow") {
        "qcow2".to_string()
    } else {
        "raw".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::validate_install_controller_mode;

    #[test]
    fn validate_install_mode_rejects_neither() {
        let err = validate_install_controller_mode(&[], false).expect_err("should fail");
        assert!(err
            .to_string()
            .contains("provide exactly one of --join-controller"));
    }

    #[test]
    fn validate_install_mode_rejects_both() {
        let err = validate_install_controller_mode(&["192.168.1.10:9090".to_string()], true)
            .expect_err("should fail");
        assert!(err
            .to_string()
            .contains("provide exactly one of --join-controller"));
    }

    #[test]
    fn validate_install_mode_accepts_join_only() {
        let join = validate_install_controller_mode(&[" 192.168.1.10:9090 ".to_string()], false)
            .expect("should pass");
        assert_eq!(join, vec!["192.168.1.10:9090"]);
    }

    #[test]
    fn validate_install_mode_accepts_run_controller_only() {
        let join = validate_install_controller_mode(&[], true).expect("should pass");
        assert!(join.is_empty());
    }

    #[test]
    fn infer_image_format_from_name_handles_qcow2_and_raw() {
        assert_eq!(super::infer_image_format_from_name("debian.qcow2"), "qcow2");
        assert_eq!(super::infer_image_format_from_name("disk.raw"), "raw");
        assert_eq!(super::infer_image_format_from_name("disk.img"), "raw");
    }

    #[test]
    fn storage_backend_to_proto_maps_supported_values() {
        assert_eq!(
            super::storage_backend_to_proto("filesystem"),
            super::node_proto::StorageBackendType::Filesystem as i32
        );
        assert_eq!(
            super::storage_backend_to_proto("lvm"),
            super::node_proto::StorageBackendType::Lvm as i32
        );
        assert_eq!(
            super::storage_backend_to_proto("zfs"),
            super::node_proto::StorageBackendType::Zfs as i32
        );
        assert_eq!(
            super::storage_backend_to_proto(""),
            super::node_proto::StorageBackendType::Unspecified as i32
        );
    }
}
