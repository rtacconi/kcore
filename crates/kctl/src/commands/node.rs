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

    // Network view (default + custom networks on this node).
    if let Ok(overview) = client
        .get_network_overview(controller_proto::GetNetworkOverviewRequest {})
        .await
        .map(|r| r.into_inner())
    {
        if let Some(node_net) = overview.nodes.iter().find(|n| n.node_id == node_id) {
            let gateway_if = if node_net.gateway_interface.trim().is_empty() {
                overview.default_gateway_interface.as_str()
            } else {
                node_net.gateway_interface.as_str()
            };
            println!("\nNetwork:");
            println!("  Gateway interface: {}", gateway_if);
            println!("  External IP:       {}", overview.default_external_ip);
            println!("  Gateway IP:        {}", overview.default_gateway_ip);
            println!("  Netmask:           {}", overview.default_internal_netmask);
            if let Some(subnet) = ipv4_subnet_from_gateway_mask(
                &overview.default_gateway_ip,
                &overview.default_internal_netmask,
            ) {
                println!("  Subnet:            {subnet}");
            }
            println!("  Network type:      nat (default)");
            println!("  DNS forwarders:    1.1.1.1, 8.8.8.8");
        }
    }

    if let Ok(networks) = client
        .list_networks(controller_proto::ListNetworksRequest {
            target_node: node_id.to_string(),
        })
        .await
        .map(|r| r.into_inner().networks)
    {
        if !networks.is_empty() {
            println!("\nConfigured networks on node:");
            for net in networks {
                let net_type = if net.network_type.is_empty() {
                    "nat".to_string()
                } else {
                    net.network_type
                };
                println!(
                    "  - {}: type={} gateway={} mask={} external={} vlan={}",
                    net.name,
                    net_type,
                    net.gateway_ip,
                    net.internal_netmask,
                    net.external_ip,
                    if net.vlan_id > 0 {
                        net.vlan_id.to_string()
                    } else {
                        "-".to_string()
                    }
                );
            }
        }
    }

    // Storage/disk inventory from node agent.
    if let Ok(storage) = client
        .get_storage_overview(controller_proto::GetStorageOverviewRequest {})
        .await
        .map(|r| r.into_inner())
    {
        if let Some(node_storage) = storage.nodes.into_iter().find(|n| n.node_id == node_id) {
            println!("\nDisks:");
            if node_storage.disks.is_empty() {
                println!("  (none reported)");
            } else {
                println!(
                    "  {:<10}  {:<22}  {:>10}  {:<8}  {:<18}  {:<20}",
                    "NAME", "PATH", "SIZE", "FSTYPE", "MOUNTPOINT", "MODEL"
                );
                for d in node_storage.disks {
                    println!(
                        "  {:<10}  {:<22}  {:>10}  {:<8}  {:<18}  {:<20}",
                        d.name, d.path, d.size, d.fstype, d.mountpoint, d.model
                    );
                }
            }
            println!("\nLVM:");
            if !node_storage.lvm_inventory_ok {
                println!("  unavailable: node did not report LVM inventory");
            } else {
                if node_storage.lvm_volume_groups.is_empty() {
                    println!("  Volume groups: (none)");
                } else {
                    println!("  Volume groups:");
                    for vg in node_storage.lvm_volume_groups {
                        println!(
                            "    - {} size={} free={} attr={}",
                            vg.name,
                            client::format_bytes(vg.size_bytes),
                            client::format_bytes(vg.free_bytes),
                            vg.attr
                        );
                    }
                }

                if node_storage.lvm_logical_volumes.is_empty() {
                    println!("  Logical volumes: (none)");
                } else {
                    println!("  Logical volumes:");
                    for lv in node_storage.lvm_logical_volumes {
                        println!(
                            "    - {}/{} size={} path={} attr={}{}{}{}{}",
                            lv.vg_name,
                            lv.name,
                            client::format_bytes(lv.size_bytes),
                            lv.path,
                            lv.attr,
                            if lv.pool.is_empty() {
                                String::new()
                            } else {
                                format!(" pool={}", lv.pool)
                            },
                            if lv.origin.is_empty() {
                                String::new()
                            } else {
                                format!(" origin={}", lv.origin)
                            },
                            if lv.data_percent.is_empty() {
                                String::new()
                            } else {
                                format!(" data%={}", lv.data_percent)
                            },
                            if lv.metadata_percent.is_empty() {
                                String::new()
                            } else {
                                format!(" meta%={}", lv.metadata_percent)
                            },
                        );
                    }
                }

                if node_storage.lvm_physical_volumes.is_empty() {
                    println!("  Physical volumes: (none)");
                } else {
                    println!("  Physical volumes:");
                    for pv in node_storage.lvm_physical_volumes {
                        println!(
                            "    - {} vg={} size={} free={} attr={}",
                            pv.name,
                            if pv.vg_name.is_empty() {
                                "-".to_string()
                            } else {
                                pv.vg_name
                            },
                            client::format_bytes(pv.size_bytes),
                            client::format_bytes(pv.free_bytes),
                            pv.attr
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

fn ipv4_subnet_from_gateway_mask(gateway_ip: &str, netmask: &str) -> Option<String> {
    fn parse(ip: &str) -> Option<[u8; 4]> {
        let mut parts = ip.split('.');
        let a = parts.next()?.parse::<u8>().ok()?;
        let b = parts.next()?.parse::<u8>().ok()?;
        let c = parts.next()?.parse::<u8>().ok()?;
        let d = parts.next()?.parse::<u8>().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some([a, b, c, d])
    }

    let ip = parse(gateway_ip)?;
    let mask = parse(netmask)?;
    let net = [
        ip[0] & mask[0],
        ip[1] & mask[1],
        ip[2] & mask[2],
        ip[3] & mask[3],
    ];
    let prefix_len = mask.iter().map(|b| b.count_ones()).sum::<u32>();
    Some(format!(
        "{}.{}.{}.{}/{}",
        net[0], net[1], net[2], net[3], prefix_len
    ))
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
    controller_info: Option<&ConnectionInfo>,
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
    config_path: &Path,
) -> Result<()> {
    let join_controllers = validate_install_controller_mode(join_controllers, run_controller)?;
    let primary_controller = join_controllers.first().cloned().unwrap_or_default();

    let node_host =
        pki::host_from_address(&info.address).map_err(|e| anyhow::anyhow!("node address: {e}"))?;

    let effective_node_id = node_id
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    let node_is_controller = run_controller;

    let install_pki = if node_is_controller {
        pki::load_install_pki(certs_dir, &node_host, true)
            .map_err(|e| anyhow::anyhow!("loading controller install PKI: {e}"))?
    } else {
        let ca_path = certs_dir.join("ca.crt");
        let ca_cert_pem = std::fs::read_to_string(&ca_path)
            .with_context(|| format!("reading {}", ca_path.display()))?;

        // For agent-only installs, ask the target controller to issue
        // the node bootstrap cert/key so cert issuance is tied to controller CA.
        // Use inline PEM data from the controller_info when available; fall back
        // to file paths from the certs_dir.
        let controller_conn = if let Some(ci) = controller_info {
            let has_inline = ci.cert_pem.is_some() && ci.key_pem.is_some() && ci.ca_pem.is_some();
            let has_files = ci.cert.is_some() && ci.key.is_some() && ci.ca.is_some();
            if has_inline || has_files {
                ConnectionInfo {
                    address: primary_controller.clone(),
                    addresses: join_controllers.clone(),
                    insecure: false,
                    tls_server_name: ci.tls_server_name.clone(),
                    cert_pem: ci.cert_pem.clone(),
                    key_pem: ci.key_pem.clone(),
                    ca_pem: ci.ca_pem.clone(),
                    cert: ci.cert.clone(),
                    key: ci.key.clone(),
                    ca: ci.ca.clone(),
                }
            } else {
                ConnectionInfo {
                    address: primary_controller.clone(),
                    addresses: join_controllers.clone(),
                    insecure: false,
                    tls_server_name: ci.tls_server_name.clone(),
                    cert_pem: None,
                    key_pem: None,
                    ca_pem: None,
                    cert: Some(certs_dir.join("kctl.crt").display().to_string()),
                    key: Some(certs_dir.join("kctl.key").display().to_string()),
                    ca: Some(certs_dir.join("ca.crt").display().to_string()),
                }
            }
        } else {
            ConnectionInfo {
                address: primary_controller.clone(),
                addresses: join_controllers.clone(),
                insecure: false,
                tls_server_name: None,
                cert_pem: None,
                key_pem: None,
                ca_pem: None,
                cert: Some(certs_dir.join("kctl.crt").display().to_string()),
                key: Some(certs_dir.join("kctl.key").display().to_string()),
                ca: Some(certs_dir.join("ca.crt").display().to_string()),
            }
        };

        let mut ctl = client::controller_client(&controller_conn)
            .await
            .with_context(|| {
                format!("connecting to controller {primary_controller} to issue bootstrap certs")
            })?;
        let bootstrap_node_id = effective_node_id
            .clone()
            .unwrap_or_else(|| format!("kcore-node-{node_host}"));
        let issued = ctl
            .issue_node_bootstrap_cert(controller_proto::IssueNodeBootstrapCertRequest {
                node_id: bootstrap_node_id.clone(),
                node_host: node_host.clone(),
            })
            .await
            .with_context(|| {
                format!("requesting node bootstrap cert from controller {primary_controller}")
            })?
            .into_inner();
        if !issued.success {
            anyhow::bail!(
                "controller refused bootstrap cert issuance: {}",
                issued.message
            );
        }

        pki::InstallPkiPayload {
            ca_cert_pem,
            node_cert_pem: issued.cert_pem,
            node_key_pem: issued.key_pem,
            controller_cert_pem: String::new(),
            controller_key_pem: String::new(),
            sub_ca_cert_pem: String::new(),
            sub_ca_key_pem: String::new(),
        }
    };

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
            node_id: effective_node_id.clone().unwrap_or_default(),
        })
        .await?
        .into_inner();

    if resp.accepted {
        println!("Install accepted: {}", resp.message);
        if run_controller {
            let new_addr = format!("{node_host}:9090");
            match crate::config::load_config(config_path) {
                Ok(mut cfg) => {
                    let ctx_name = cfg
                        .current_context
                        .clone()
                        .or_else(|| cfg.contexts.keys().next().cloned());
                    match ctx_name {
                        Some(name) => {
                            if let Some(ctx) = cfg.contexts.get_mut(&name) {
                                if !ctx.controllers.iter().any(|c| c == &new_addr) {
                                    ctx.controllers.push(new_addr.clone());
                                    match crate::config::save_config(config_path, &cfg) {
                                        Ok(()) => println!("Added {new_addr} to kctl controllers list in context '{name}'"),
                                        Err(e) => eprintln!("Warning: failed to save kctl config after adding controller: {e}"),
                                    }
                                }
                            }
                        }
                        None => eprintln!("Warning: no kctl context found; add {new_addr} to your config manually"),
                    }
                }
                Err(e) => {
                    eprintln!("Warning: could not load kctl config to add new controller: {e}")
                }
            }
        }
        Ok(())
    } else {
        anyhow::bail!("Install rejected: {}", resp.message);
    }
}

fn normalize_controller_endpoint(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.parse::<std::net::SocketAddr>().is_ok() {
        return trimmed.to_string();
    }
    if trimmed.starts_with('[') {
        return format!("{trimmed}:9090");
    }
    let colon_count = trimmed.chars().filter(|&c| c == ':').count();
    if colon_count > 1 {
        return format!("[{trimmed}]:9090");
    }
    if colon_count == 1 {
        return trimmed.to_string();
    }
    format!("{trimmed}:9090")
}

fn validate_install_controller_mode(
    join_controllers: &[String],
    run_controller: bool,
) -> Result<Vec<String>> {
    let normalized: Vec<String> = join_controllers
        .iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| normalize_controller_endpoint(&v))
        .collect();
    let has_join = !normalized.is_empty();
    if !has_join && !run_controller {
        anyhow::bail!(
            "provide --join-controller <host:port> or --run-controller \
             (or both for a new controller joining an existing cluster)"
        );
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

pub async fn apply_disko_layout(
    info: &ConnectionInfo,
    file: &str,
    apply: bool,
    timeout_seconds: i32,
) -> Result<()> {
    let content = std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?;
    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .apply_disko_layout(node_proto::ApplyDiskoLayoutRequest {
            disko_nix: content,
            apply,
            timeout_seconds,
        })
        .await?
        .into_inner();
    if resp.success {
        println!("{}", resp.message);
        println!("disko mode: {}", resp.mode);
        Ok(())
    } else {
        anyhow::bail!("disko apply failed (mode={}): {}", resp.mode, resp.message)
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
        assert!(err.to_string().contains("provide --join-controller"));
    }

    #[test]
    fn validate_install_mode_accepts_both() {
        let join = validate_install_controller_mode(&["192.168.1.10:9090".to_string()], true)
            .expect("should pass when both flags are set");
        assert_eq!(join, vec!["192.168.1.10:9090"]);
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
    fn validate_install_mode_normalizes_port() {
        let join = validate_install_controller_mode(
            &["192.168.1.10".to_string(), "10.0.0.5:7777".to_string()],
            false,
        )
        .expect("should pass");
        assert_eq!(join, vec!["192.168.1.10:9090", "10.0.0.5:7777"]);
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
