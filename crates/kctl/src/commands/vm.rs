use std::path::Path;
use std::time::{Duration, Instant};

use crate::client::{self, controller_proto as proto};
use crate::commands::node;
use crate::config::ConnectionInfo;
use crate::output;
use anyhow::{bail, Context, Result};

pub struct CreateArgs {
    pub name: Option<String>,
    pub filename: Option<String>,
    pub cpu: i32,
    pub memory: String,
    pub image: Option<String>,
    pub image_sha256: Option<String>,
    pub image_path: Option<String>,
    pub image_format: Option<String>,
    pub network: Option<String>,
    pub target_node: Option<String>,
    pub wait: bool,
    pub wait_for_ssh: bool,
    pub wait_timeout_seconds: u64,
    pub ssh_port: i32,
    pub ssh_probe_timeout_ms: i32,
    pub ssh_keys: Vec<String>,
    pub ssh_public_keys: Vec<String>,
    pub cloud_init_user_data_file: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub compliant: bool,
    pub storage_backend: Option<String>,
    pub storage_size_bytes: Option<i64>,
    pub target_dc: Option<String>,
}

pub async fn create(info: &ConnectionInfo, args: CreateArgs) -> Result<()> {
    let has_target_node = args
        .target_node
        .as_deref()
        .is_some_and(|v| !v.trim().is_empty());
    let has_target_dc = args
        .target_dc
        .as_deref()
        .is_some_and(|v| !v.trim().is_empty());
    if has_target_node && has_target_dc {
        bail!("--target-node and --target-dc are mutually exclusive");
    }
    if args.wait_for_ssh && args.ssh_port <= 0 {
        bail!("--ssh-port must be > 0 when using --wait-for-ssh");
    }
    if (args.wait || args.wait_for_ssh) && args.wait_timeout_seconds == 0 {
        bail!("--wait-timeout-seconds must be > 0");
    }
    let cli_storage_backend = args.storage_backend.clone();
    let cli_storage_size_bytes = args.storage_size_bytes;
    if args.password.is_some() && args.username.is_none() {
        bail!("--password requires --username");
    }
    if !args.ssh_public_keys.is_empty() && args.username.is_none() {
        bail!("--ssh-public-key requires --username");
    }
    let (
        vm_name,
        vm_cpu,
        mem_bytes,
        nics,
        manifest_image,
        manifest_image_sha256,
        manifest_image_format,
        manifest_storage_backend,
        manifest_storage_size_bytes,
    ) = if let Some(path) = &args.filename {
        let manifest = parse_vm_manifest(path)?;
        let n = args.name.clone().unwrap_or(manifest.name);
        (
            n,
            manifest.cpu,
            manifest.memory_bytes,
            manifest.nics,
            manifest.image,
            manifest.image_sha256,
            manifest.image_format,
            manifest.storage_backend,
            manifest.storage_size_bytes,
        )
    } else {
        let n = args
            .name
            .clone()
            .context("NAME required (or use -f to create from a manifest)")?;
        let mem = client::parse_size_bytes(&args.memory).map_err(|e| anyhow::anyhow!(e))?;
        let cpu = args.cpu;
        let nics = args
            .network
            .as_ref()
            .map(|net| {
                vec![proto::Nic {
                    network: net.clone(),
                    model: "virtio".to_string(),
                    mac_address: String::new(),
                }]
            })
            .unwrap_or_default();
        (n, cpu, mem, nics, None, None, None, None, None)
    };
    let image = resolve_create_image_source(
        args.image.as_deref(),
        args.image_sha256.as_deref(),
        args.image_path.as_deref(),
        args.image_format.as_deref(),
        manifest_image.as_deref(),
        manifest_image_sha256.as_deref(),
        manifest_image_format.as_deref(),
    )?;
    let cloud_init_user_data = prepare_cloud_init_user_data(&vm_name, &args)?;
    if !cloud_init_user_data.is_empty() && !args.ssh_keys.is_empty() {
        bail!(
            "cannot combine --ssh-key with custom/generated cloud-init user-data; include SSH keys in cloud-init or omit --cloud-init-user-data-file/--username/--password"
        );
    }

    let storage_backend_raw = cli_storage_backend
        .or(manifest_storage_backend)
        .unwrap_or_else(|| "filesystem".to_string());
    let storage_backend = normalize_storage_backend_arg(&storage_backend_raw)?;
    let storage_size_bytes = cli_storage_size_bytes
        .or(manifest_storage_size_bytes)
        .unwrap_or(0);
    if storage_size_bytes <= 0 {
        bail!(
            "--storage-size-bytes must be > 0 (set via CLI flag or spec.storageSizeBytes in YAML)"
        );
    }

    let mut client = client::controller_client(info).await?;

    let spec = proto::VmSpec {
        id: String::new(),
        name: vm_name.clone(),
        cpu: vm_cpu,
        memory_bytes: mem_bytes,
        disks: vec![],
        nics,
    };

    let req = proto::CreateVmRequest {
        target_node: args.target_node.unwrap_or_default(),
        spec: Some(spec),
        image_url: image.url,
        image_sha256: image.sha256,
        cloud_init_user_data,
        image_path: image.path,
        image_format: image.format,
        ssh_key_names: args.ssh_keys,
        storage_backend: storage_backend_to_proto(&storage_backend),
        storage_size_bytes,
        target_dc: args.target_dc.unwrap_or_default(),
    };

    let resp = client.create_vm(req).await?.into_inner();

    println!("VM '{vm_name}' created");
    println!("  ID:   {}", resp.vm_id);
    println!("  Node: {}", resp.node_id);
    println!("  CPU:  {vm_cpu} cores");
    println!("  Mem:  {}", client::format_bytes(mem_bytes));

    if args.wait || args.wait_for_ssh {
        let mode = if args.wait_for_ssh {
            WaitMode::RunningAndSsh
        } else {
            WaitMode::RunningOnly
        };
        wait_for_vm_readiness(
            info,
            &resp.vm_id,
            Duration::from_secs(args.wait_timeout_seconds),
            mode,
            args.ssh_port,
            args.ssh_probe_timeout_ms,
        )
        .await?;
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WaitMode {
    RunningOnly,
    RunningAndSsh,
}

async fn wait_for_vm_readiness(
    info: &ConnectionInfo,
    vm_id: &str,
    timeout: Duration,
    mode: WaitMode,
    ssh_port: i32,
    ssh_probe_timeout_ms: i32,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut node_address_cache: Option<String> = None;
    loop {
        if Instant::now() > deadline {
            bail!(
                "timeout waiting for VM {} readiness after {} seconds",
                vm_id,
                timeout.as_secs()
            );
        }

        let mut controller = client::controller_client(info).await?;
        let get = controller
            .get_vm(proto::GetVmRequest {
                vm_id: vm_id.to_string(),
                target_node: String::new(),
            })
            .await?
            .into_inner();
        let spec = get.spec.as_ref().context("get_vm missing spec")?;
        let status = get.status.as_ref().context("get_vm missing status")?;
        let current_state =
            proto::VmState::try_from(status.state).unwrap_or(proto::VmState::Unknown);

        if mode == WaitMode::RunningAndSsh {
            if node_address_cache.is_none() {
                let mut controller = client::controller_client(info).await?;
                let nodes = controller
                    .list_nodes(proto::ListNodesRequest {})
                    .await?
                    .into_inner()
                    .nodes;
                node_address_cache = node_address_for_vm_node_id(&nodes, &get.node_id);
            }
            if let Some(node_address) = node_address_cache.clone() {
                let node_info = ConnectionInfo {
                    address: node_address,
                    addresses: vec![],
                    insecure: info.insecure,
                    tls_server_name: None,
                    cert_pem: info.cert_pem.clone(),
                    key_pem: info.key_pem.clone(),
                    ca_pem: info.ca_pem.clone(),
                    cert: info.cert.clone(),
                    key: info.key.clone(),
                    ca: info.ca.clone(),
                };
                let ssh = node::check_vm_ssh_ready(
                    &node_info,
                    &spec.name,
                    None,
                    ssh_port,
                    ssh_probe_timeout_ms,
                )
                .await?;
                if ssh.fatal {
                    bail!(
                        "VM '{}' reached fatal readiness state: {}",
                        spec.name,
                        ssh.reason
                    );
                }
                if current_state == proto::VmState::Running && ssh.ready {
                    println!("VM '{}' SSH is ready at {}:{}", spec.name, ssh.ip, ssh.port);
                    return Ok(());
                }
                if current_state == proto::VmState::Running && !ssh.reason.is_empty() {
                    println!(
                        "waiting for SSH on VM '{}': {}{}",
                        spec.name,
                        ssh.reason,
                        if ssh.ip.is_empty() {
                            String::new()
                        } else {
                            format!(" (candidate ip: {})", ssh.ip)
                        }
                    );
                }
            }
        } else if current_state == proto::VmState::Running {
            println!("VM '{}' is running", spec.name);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn node_address_for_vm_node_id(nodes: &[proto::NodeInfo], node_id: &str) -> Option<String> {
    nodes
        .iter()
        .find(|n| n.node_id == node_id)
        .map(|n| n.address.clone())
}

pub async fn delete(info: &ConnectionInfo, vm_id: &str, target_node: Option<String>) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    client
        .delete_vm(proto::DeleteVmRequest {
            vm_id: vm_id.to_string(),
            target_node: target_node.unwrap_or_default(),
        })
        .await?;
    println!("VM '{vm_id}' deleted");
    Ok(())
}

pub async fn update(
    info: &ConnectionInfo,
    vm_id: &str,
    cpu: Option<i32>,
    memory: Option<String>,
    target_node: Option<String>,
) -> Result<()> {
    let memory_bytes = match &memory {
        Some(m) => client::parse_size_bytes(m).map_err(|e| anyhow::anyhow!(e))?,
        None => 0,
    };

    let mut client = client::controller_client(info).await?;
    let resp = client
        .update_vm(proto::UpdateVmRequest {
            vm_id: vm_id.to_string(),
            target_node: target_node.unwrap_or_default(),
            cpu: cpu.unwrap_or(0),
            memory_bytes,
        })
        .await?
        .into_inner();

    if resp.success {
        println!("{}", resp.message);
        Ok(())
    } else {
        anyhow::bail!("Update failed: {}", resp.message);
    }
}

pub async fn start(info: &ConnectionInfo, vm_id: &str, target_node: Option<String>) -> Result<()> {
    // Legacy alias for set desired-state=running.
    set_desired_state(
        info,
        vm_id,
        proto::VmDesiredState::Running,
        target_node,
        "running",
    )
    .await
}

pub async fn stop(info: &ConnectionInfo, vm_id: &str, target_node: Option<String>) -> Result<()> {
    // Legacy alias for set desired-state=stopped.
    set_desired_state(
        info,
        vm_id,
        proto::VmDesiredState::Stopped,
        target_node,
        "stopped",
    )
    .await
}

pub async fn set_desired_state(
    info: &ConnectionInfo,
    vm_id: &str,
    desired_state: proto::VmDesiredState,
    target_node: Option<String>,
    state_label: &str,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let req = build_set_vm_desired_state_request(vm_id, desired_state, target_node);
    client.set_vm_desired_state(req).await?;
    println!("VM '{vm_id}' desired state set to {state_label} (declarative apply started)");
    Ok(())
}

fn build_set_vm_desired_state_request(
    vm_id: &str,
    desired_state: proto::VmDesiredState,
    target_node: Option<String>,
) -> proto::SetVmDesiredStateRequest {
    proto::SetVmDesiredStateRequest {
        vm_id: vm_id.to_string(),
        desired_state: desired_state as i32,
        target_node: target_node.unwrap_or_default(),
    }
}

pub async fn get(info: &ConnectionInfo, vm_id: &str, target_node: Option<String>) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .get_vm(proto::GetVmRequest {
            vm_id: vm_id.to_string(),
            target_node: target_node.unwrap_or_default(),
        })
        .await?
        .into_inner();

    let spec = resp.spec.as_ref().context("no spec in response")?;
    let status = resp.status.as_ref().context("no status in response")?;
    output::print_vm_detail(spec, status, &resp.node_id);

    Ok(())
}

pub async fn describe(
    info: &ConnectionInfo,
    vm_id: &str,
    target_node: Option<String>,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .get_vm(proto::GetVmRequest {
            vm_id: vm_id.to_string(),
            target_node: target_node.unwrap_or_default(),
        })
        .await?
        .into_inner();

    let spec = resp.spec.as_ref().context("no spec in response")?;
    let status = resp.status.as_ref().context("no status in response")?;
    output::print_vm_detail(spec, status, &resp.node_id);

    if spec.nics.is_empty() {
        return Ok(());
    }

    let mut controller = client::controller_client(info).await?;
    let networks = controller
        .list_networks(proto::ListNetworksRequest {
            target_node: resp.node_id.clone(),
        })
        .await?
        .into_inner()
        .networks;
    let default_network_overview = controller
        .get_network_overview(proto::GetNetworkOverviewRequest {})
        .await
        .ok()
        .map(|r| r.into_inner());

    let primary_network = spec
        .nics
        .first()
        .map(|n| n.network.clone())
        .filter(|s| !s.trim().is_empty());
    let primary_network_type = primary_network
        .as_deref()
        .map(|name| network_type_for(name, &resp.node_id, &networks));

    let nodes = controller
        .list_nodes(proto::ListNodesRequest {})
        .await?
        .into_inner()
        .nodes;
    let mut probed_ip: Option<String> = None;
    let mut probe_reason: Option<String> = None;
    if let Some(node_address) = node_address_for_vm_node_id(&nodes, &resp.node_id) {
        let node_info = ConnectionInfo {
            address: node_address,
            addresses: vec![],
            insecure: info.insecure,
            tls_server_name: None,
            cert_pem: info.cert_pem.clone(),
            key_pem: info.key_pem.clone(),
            ca_pem: info.ca_pem.clone(),
            cert: info.cert.clone(),
            key: info.key.clone(),
            ca: info.ca.clone(),
        };
        if let Ok(ssh_probe) =
            node::check_vm_ssh_ready(&node_info, &spec.name, primary_network.as_deref(), 22, 1200)
                .await
        {
            if !ssh_probe.ip.is_empty() {
                probed_ip = Some(ssh_probe.ip);
            } else if !ssh_probe.reason.is_empty() {
                probe_reason = Some(ssh_probe.reason);
            }
        }
    }

    if let Some(ip) = probed_ip {
        println!("IP:       {}", ip);
    } else if !resp.assigned_ip.trim().is_empty() {
        if primary_network_type.as_deref() == Some("vxlan") {
            println!("IP:       {} (static vxlan assignment)", resp.assigned_ip);
        } else {
            println!("IP:       {} (controller assignment)", resp.assigned_ip);
        }
    } else if let Some(reason) = probe_reason {
        println!("IP:       unavailable ({reason})");
    } else {
        println!("IP:       unavailable (no address strategy produced an IP)");
    }

    println!("\nNetwork configuration:");
    for nic in &spec.nics {
        println!(
            "  NIC: network={} model={} mac={}",
            nic.network,
            if nic.model.is_empty() {
                "virtio"
            } else {
                nic.model.as_str()
            },
            if nic.mac_address.is_empty() {
                "(auto)"
            } else {
                nic.mac_address.as_str()
            }
        );

        if let Some(net) = networks
            .iter()
            .find(|n| n.name == nic.network && n.node_id == resp.node_id)
        {
            let net_type = if net.network_type.is_empty() {
                "nat".to_string()
            } else {
                net.network_type.clone()
            };
            println!("    network_type:   {net_type}");
            println!("    gateway_ip:     {}", net.gateway_ip);
            println!("    netmask:        {}", net.internal_netmask);
            println!("    external_ip:    {}", net.external_ip);
            println!(
                "    outbound_nat:   {}",
                if net.enable_outbound_nat {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!(
                "    allowed_tcp:    {}",
                if net.allowed_tcp_ports.is_empty() {
                    "(none)".to_string()
                } else {
                    net.allowed_tcp_ports
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            );
            println!(
                "    allowed_udp:    {}",
                if net.allowed_udp_ports.is_empty() {
                    "(none)".to_string()
                } else {
                    net.allowed_udp_ports
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            );
        } else {
            if nic.network == "default" {
                if let Some(overview) = &default_network_overview {
                    println!("    network_type:   nat");
                    println!("    gateway_ip:     {}", overview.default_gateway_ip);
                    println!("    netmask:        {}", overview.default_internal_netmask);
                    println!("    external_ip:    {}", overview.default_external_ip);
                    println!("    outbound_nat:   enabled");
                    println!("    allowed_tcp:    (cluster defaults)");
                    println!("    allowed_udp:    (cluster defaults)");
                } else {
                    println!("    details:        default network overview unavailable");
                }
            } else {
                println!("    details:        not found in controller network inventory");
            }
        }
    }

    Ok(())
}

fn network_type_for(network_name: &str, node_id: &str, networks: &[proto::NetworkInfo]) -> String {
    if network_name == "default" {
        return "nat".to_string();
    }
    networks
        .iter()
        .find(|n| n.name == network_name && n.node_id == node_id)
        .map(|n| {
            if n.network_type.trim().is_empty() {
                "nat".to_string()
            } else {
                n.network_type.trim().to_string()
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

pub async fn list(info: &ConnectionInfo, target_node: Option<String>) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .list_vms(proto::ListVmsRequest {
            target_node: target_node.unwrap_or_default(),
        })
        .await?
        .into_inner();

    if resp.vms.is_empty() {
        println!("No VMs found");
        return Ok(());
    }

    output::print_vm_table(&resp.vms);
    Ok(())
}

struct VmManifest {
    name: String,
    cpu: i32,
    memory_bytes: i64,
    nics: Vec<proto::Nic>,
    image: Option<String>,
    image_sha256: Option<String>,
    image_format: Option<String>,
    storage_backend: Option<String>,
    storage_size_bytes: Option<i64>,
}

#[derive(Debug)]
struct ImageSource {
    url: String,
    sha256: String,
    path: String,
    format: String,
}

fn prepare_cloud_init_user_data(vm_name: &str, args: &CreateArgs) -> Result<String> {
    let has_explicit_cloud_init = args.cloud_init_user_data_file.is_some();
    let has_identity_overrides =
        args.username.is_some() || args.password.is_some() || !args.ssh_public_keys.is_empty();

    if has_explicit_cloud_init && has_identity_overrides {
        bail!(
            "--cloud-init-user-data-file cannot be combined with --username/--password/--ssh-public-key"
        );
    }

    if let Some(path) = &args.cloud_init_user_data_file {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading cloud-init user-data file: {path}"))?;
        if content.trim().is_empty() {
            bail!("cloud-init user-data file is empty: {path}");
        }
        return Ok(content);
    }

    if !has_identity_overrides {
        return Ok(String::new());
    }

    let username = args
        .username
        .as_deref()
        .ok_or_else(|| {
            anyhow::anyhow!("--username is required when overriding cloud-init identity")
        })?
        .trim();
    if username.is_empty() {
        bail!("--username must not be empty");
    }
    if username.contains(char::is_whitespace) {
        bail!("--username must not contain whitespace");
    }

    let password = args.password.as_deref().unwrap_or("").trim();
    if !password.is_empty() && args.compliant {
        bail!(
            "password-based VM access is non-compliant; rerun with --compliant=false to acknowledge risk"
        );
    }
    if password.is_empty() && args.ssh_public_keys.is_empty() {
        bail!(
            "identity override requires at least one --ssh-public-key, or --password with --compliant=false"
        );
    }

    let mut out = String::new();
    out.push_str("#cloud-config\n");
    out.push_str(&format!("hostname: {vm_name}\n"));
    out.push_str("users:\n");
    out.push_str("  - default\n");
    out.push_str(&format!("  - name: {username}\n"));
    out.push_str("    gecos: kcore VM user\n");
    out.push_str("    groups: [sudo]\n");
    out.push_str("    shell: /bin/bash\n");
    if password.is_empty() {
        out.push_str("    lock_passwd: true\n");
    } else {
        out.push_str("    lock_passwd: false\n");
    }
    if !args.ssh_public_keys.is_empty() {
        out.push_str("    ssh_authorized_keys:\n");
        for key in &args.ssh_public_keys {
            let trimmed = key.trim();
            if !trimmed.starts_with("ssh-") {
                bail!("invalid --ssh-public-key value (must start with ssh-): {trimmed}");
            }
            out.push_str(&format!(
                "      - \"{}\"\n",
                yaml_escape_double_quoted(trimmed)
            ));
        }
        out.push_str("ssh_pwauth: false\n");
    } else {
        out.push_str("ssh_pwauth: true\n");
    }
    if !password.is_empty() {
        out.push_str("chpasswd:\n");
        out.push_str("  expire: false\n");
        out.push_str("  users:\n");
        out.push_str(&format!("    - name: {username}\n"));
        out.push_str(&format!(
            "      password: \"{}\"\n",
            yaml_escape_double_quoted(password)
        ));
    }

    Ok(out)
}

fn yaml_escape_double_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn resolve_create_image_source(
    cli_image: Option<&str>,
    cli_image_sha256: Option<&str>,
    cli_image_path: Option<&str>,
    cli_image_format: Option<&str>,
    manifest_image: Option<&str>,
    manifest_image_sha256: Option<&str>,
    manifest_image_format: Option<&str>,
) -> Result<ImageSource> {
    let selected = select_image_mode(
        cli_image,
        cli_image_path,
        manifest_image,
        manifest_image_sha256,
    )?;
    if selected == "url" {
        let url = cli_image.or(manifest_image).unwrap_or("").trim();
        let sha256 = cli_image_sha256
            .or(manifest_image_sha256)
            .unwrap_or("")
            .trim();
        if !url.starts_with("https://") {
            bail!("VM image must be an https:// URL, got: {url}");
        }
        if sha256.is_empty() {
            bail!("VM image SHA256 is required. Pass --image-sha256 <hex> or set spec.imageSha256 in the VM manifest.");
        }
        if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("VM image SHA256 must be exactly 64 hexadecimal characters.");
        }
        return Ok(ImageSource {
            url: url.to_string(),
            sha256: sha256.to_ascii_lowercase(),
            path: String::new(),
            format: String::new(),
        });
    }

    let path = cli_image_path
        .or_else(|| {
            manifest_image.and_then(|img| {
                if img.starts_with("https://") {
                    None
                } else {
                    Some(img)
                }
            })
        })
        .unwrap_or("")
        .trim();
    if path.is_empty() {
        bail!("VM image path is required when using local image mode");
    }
    let format = normalize_image_format(
        cli_image_format
            .or(manifest_image_format)
            .unwrap_or_else(|| infer_format_from_path(path)),
    )?;
    Ok(ImageSource {
        url: String::new(),
        sha256: String::new(),
        path: path.to_string(),
        format,
    })
}

fn select_image_mode(
    cli_image: Option<&str>,
    cli_image_path: Option<&str>,
    manifest_image: Option<&str>,
    manifest_image_sha256: Option<&str>,
) -> Result<&'static str> {
    let cli_has_url = cli_image.is_some_and(|s| !s.trim().is_empty());
    let cli_has_path = cli_image_path.is_some_and(|s| !s.trim().is_empty());
    if cli_has_url && cli_has_path {
        bail!("--image and --image-path are mutually exclusive");
    }
    if cli_has_url {
        return Ok("url");
    }
    if cli_has_path {
        return Ok("path");
    }
    if let Some(img) = manifest_image {
        if img.trim().starts_with("https://") {
            return Ok("url");
        }
        if !img.trim().is_empty() {
            return Ok("path");
        }
    }
    if manifest_image_sha256.is_some_and(|s| !s.trim().is_empty()) {
        return Ok("url");
    }
    bail!(
        "VM image is required. Use --image <https-url> with --image-sha256, or --image-path <node-local-path>."
    )
}

fn infer_format_from_path(path: &str) -> &str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".qcow2") || lower.ends_with(".qcow") {
        "qcow2"
    } else {
        "raw"
    }
}

fn normalize_image_format(format: &str) -> Result<String> {
    let normalized = format.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "raw" | "qcow2" => Ok(normalized),
        _ => bail!("image format must be 'raw' or 'qcow2'"),
    }
}

fn normalize_storage_backend_arg(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "filesystem" | "lvm" | "zfs" => Ok(normalized),
        _ => bail!("storage backend must be one of: filesystem, lvm, zfs"),
    }
}

fn storage_backend_to_proto(value: &str) -> i32 {
    match value {
        "filesystem" => proto::StorageBackendType::Filesystem as i32,
        "lvm" => proto::StorageBackendType::Lvm as i32,
        "zfs" => proto::StorageBackendType::Zfs as i32,
        _ => proto::StorageBackendType::Unspecified as i32,
    }
}

fn parse_vm_manifest(path: &str) -> Result<VmManifest> {
    let data = std::fs::read_to_string(Path::new(path))?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&data)?;

    let kind = doc["kind"].as_str().unwrap_or("");
    if kind != "VM" {
        bail!("expected kind=VM, got {kind}");
    }

    let name = doc["metadata"]["name"]
        .as_str()
        .unwrap_or("unnamed")
        .to_string();

    let cpu = doc["spec"]["cpu"].as_i64().unwrap_or(2) as i32;

    let mem_str = doc["spec"]["memoryBytes"].as_str().unwrap_or("2G");
    let memory_bytes = client::parse_size_bytes(mem_str).map_err(|e| anyhow::anyhow!(e))?;

    let nics = doc["spec"]["nics"]
        .as_sequence()
        .map(|seq| {
            seq.iter()
                .map(|n| proto::Nic {
                    network: n["network"].as_str().unwrap_or("default").to_string(),
                    model: n["model"].as_str().unwrap_or("virtio").to_string(),
                    mac_address: String::new(),
                })
                .collect()
        })
        .unwrap_or_default();

    let image = doc["spec"]["disks"]
        .as_sequence()
        .and_then(|seq| seq.first())
        .and_then(|disk| {
            disk["backendHandle"]
                .as_str()
                .or_else(|| disk["backend_handle"].as_str())
                .or_else(|| disk["path"].as_str())
                .or_else(|| disk["image"].as_str())
        })
        .map(|s| s.to_string());
    let image_sha256 = doc["spec"]["imageSha256"]
        .as_str()
        .or_else(|| doc["spec"]["image_sha256"].as_str())
        .or_else(|| {
            doc["spec"]["disks"]
                .as_sequence()
                .and_then(|seq| seq.first())
                .and_then(|disk| {
                    disk["sha256"]
                        .as_str()
                        .or_else(|| disk["checksum"].as_str())
                })
        })
        .map(|s| s.to_string());
    let image_format = doc["spec"]["imageFormat"]
        .as_str()
        .or_else(|| doc["spec"]["image_format"].as_str())
        .or_else(|| {
            doc["spec"]["disks"]
                .as_sequence()
                .and_then(|seq| seq.first())
                .and_then(|disk| {
                    disk["format"]
                        .as_str()
                        .or_else(|| disk["imageFormat"].as_str())
                        .or_else(|| disk["image_format"].as_str())
                })
        })
        .map(|s| s.to_string());

    let storage_backend = doc["spec"]["storageBackend"]
        .as_str()
        .or_else(|| doc["spec"]["storage_backend"].as_str())
        .map(|s| s.to_string());
    let storage_size_bytes = doc["spec"]["storageSizeBytes"]
        .as_i64()
        .or_else(|| doc["spec"]["storage_size_bytes"].as_i64())
        .or_else(|| {
            doc["spec"]["storageSizeBytes"]
                .as_str()
                .or_else(|| doc["spec"]["storage_size_bytes"].as_str())
                .and_then(|s| client::parse_size_bytes(s).ok())
        });

    Ok(VmManifest {
        name,
        cpu,
        memory_bytes,
        nics,
        image,
        image_sha256,
        image_format,
        storage_backend,
        storage_size_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_set_vm_desired_state_request_sets_running_and_target_node() {
        let req = build_set_vm_desired_state_request(
            "web-1",
            proto::VmDesiredState::Running,
            Some("node-a".to_string()),
        );
        assert_eq!(req.vm_id, "web-1");
        assert_eq!(req.desired_state, proto::VmDesiredState::Running as i32);
        assert_eq!(req.target_node, "node-a");
    }

    #[test]
    fn build_set_vm_desired_state_request_defaults_empty_target_node() {
        let req = build_set_vm_desired_state_request("web-1", proto::VmDesiredState::Stopped, None);
        assert_eq!(req.vm_id, "web-1");
        assert_eq!(req.desired_state, proto::VmDesiredState::Stopped as i32);
        assert!(req.target_node.is_empty());
    }

    #[test]
    fn resolve_create_image_prefers_cli_argument() {
        let image = resolve_create_image_source(
            Some("https://example.com/cli.img"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            None,
            None,
            Some("https://example.com/manifest.img"),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            None,
        )
        .expect("image");
        assert_eq!(image.url, "https://example.com/cli.img");
        assert_eq!(
            image.sha256,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn resolve_create_image_uses_manifest_when_cli_missing() {
        let image = resolve_create_image_source(
            None,
            None,
            None,
            None,
            Some("https://example.com/manifest.img"),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            None,
        )
        .expect("image");
        assert_eq!(image.url, "https://example.com/manifest.img");
    }

    #[test]
    fn resolve_create_image_rejects_missing_image() {
        let err = resolve_create_image_source(None, None, None, None, None, None, None)
            .expect_err("missing image should fail");
        assert!(err.to_string().contains("VM image is required"));
    }

    #[test]
    fn resolve_create_image_rejects_non_https_and_missing_sha() {
        let err = resolve_create_image_source(
            Some("http://example.com/debian.img"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect_err("http image should fail");
        assert!(err.to_string().contains("https:// URL"));
    }

    #[test]
    fn resolve_create_image_accepts_local_path_mode() {
        let image = resolve_create_image_source(
            None,
            None,
            Some("/var/lib/kcore/images/debian.raw"),
            Some("raw"),
            None,
            None,
            None,
        )
        .expect("local image");
        assert!(image.url.is_empty());
        assert_eq!(image.path, "/var/lib/kcore/images/debian.raw");
        assert_eq!(image.format, "raw");
    }

    #[test]
    fn node_address_for_vm_node_id_picks_matching_node() {
        let nodes = vec![
            proto::NodeInfo {
                node_id: "node-a".to_string(),
                hostname: "a".to_string(),
                address: "10.0.0.1:9091".to_string(),
                capacity: None,
                usage: None,
                status: "ready".to_string(),
                last_heartbeat: None,
                labels: vec![],
                storage_backend: proto::StorageBackendType::Filesystem as i32,
                disable_vxlan: false,
                approval_status: "approved".to_string(),
                cert_expiry_days: -1,
                luks_method: String::new(),
                dc_id: String::new(),
            },
            proto::NodeInfo {
                node_id: "node-b".to_string(),
                hostname: "b".to_string(),
                address: "10.0.0.2:9091".to_string(),
                capacity: None,
                usage: None,
                status: "ready".to_string(),
                last_heartbeat: None,
                labels: vec![],
                storage_backend: proto::StorageBackendType::Filesystem as i32,
                disable_vxlan: false,
                approval_status: "approved".to_string(),
                cert_expiry_days: -1,
                luks_method: String::new(),
                dc_id: String::new(),
            },
        ];
        let addr = node_address_for_vm_node_id(&nodes, "node-b");
        assert_eq!(addr.as_deref(), Some("10.0.0.2:9091"));
        assert!(node_address_for_vm_node_id(&nodes, "missing").is_none());
    }

    #[test]
    fn resolve_create_image_rejects_url_and_path_together() {
        let err = resolve_create_image_source(
            Some("https://example.com/debian.qcow2"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Some("/var/lib/kcore/images/debian.qcow2"),
            Some("qcow2"),
            None,
            None,
            None,
        )
        .expect_err("mixed modes should fail");
        assert!(err.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn resolve_create_image_rejects_invalid_local_image_format() {
        let err = resolve_create_image_source(
            None,
            None,
            Some("/var/lib/kcore/images/debian.img"),
            Some("iso"),
            None,
            None,
            None,
        )
        .expect_err("unsupported format should fail");
        assert!(err.to_string().contains("raw"));
        assert!(err.to_string().contains("qcow2"));
    }

    #[test]
    fn normalize_storage_backend_arg_accepts_supported_values() {
        assert_eq!(
            normalize_storage_backend_arg("filesystem").expect("filesystem"),
            "filesystem"
        );
        assert_eq!(normalize_storage_backend_arg("LVM").expect("lvm"), "lvm");
        assert_eq!(normalize_storage_backend_arg("zfs").expect("zfs"), "zfs");
        assert!(normalize_storage_backend_arg("ceph").is_err());
    }

    fn base_create_args() -> CreateArgs {
        CreateArgs {
            name: Some("vm-a".into()),
            filename: None,
            cpu: 2,
            memory: "2G".into(),
            image: Some("https://example.com/debian.qcow2".into()),
            image_sha256: Some(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ),
            image_path: None,
            image_format: None,
            network: None,
            target_node: None,
            wait: false,
            wait_for_ssh: false,
            wait_timeout_seconds: 300,
            ssh_port: 22,
            ssh_probe_timeout_ms: 1500,
            ssh_keys: vec![],
            ssh_public_keys: vec![],
            cloud_init_user_data_file: None,
            username: None,
            password: None,
            compliant: true,
            storage_backend: Some("filesystem".into()),
            storage_size_bytes: Some(10 * 1024 * 1024 * 1024),
            target_dc: None,
        }
    }

    #[test]
    fn prepare_cloud_init_rejects_password_in_compliant_mode() {
        let mut args = base_create_args();
        args.username = Some("alice".into());
        args.password = Some("secret".into());
        let err = prepare_cloud_init_user_data("vm-a", &args).expect_err("must reject");
        assert!(err.to_string().contains("non-compliant"));
    }

    #[test]
    fn prepare_cloud_init_builds_passwordless_key_auth() {
        let mut args = base_create_args();
        args.username = Some("alice".into());
        args.ssh_public_keys = vec!["ssh-ed25519 AAAA test@example".into()];
        let data = prepare_cloud_init_user_data("vm-a", &args).expect("cloud-init");
        assert!(data.contains("name: alice"));
        assert!(data.contains("ssh_authorized_keys"));
        assert!(data.contains("lock_passwd: true"));
        assert!(data.contains("ssh_pwauth: false"));
    }

    #[test]
    fn network_type_for_defaults_to_nat_for_default_network() {
        let networks = vec![];
        assert_eq!(network_type_for("default", "node-a", &networks), "nat");
    }

    #[test]
    fn network_type_for_uses_inventory_value_or_unknown() {
        let networks = vec![proto::NetworkInfo {
            name: "vxlan-test".to_string(),
            external_ip: "192.168.1.10".to_string(),
            gateway_ip: "10.241.0.1".to_string(),
            internal_netmask: "255.255.255.0".to_string(),
            node_id: "node-a".to_string(),
            allowed_tcp_ports: vec![],
            allowed_udp_ports: vec![],
            vlan_id: 0,
            network_type: "vxlan".to_string(),
            enable_outbound_nat: true,
        }];
        assert_eq!(network_type_for("vxlan-test", "node-a", &networks), "vxlan");
        assert_eq!(network_type_for("missing", "node-a", &networks), "unknown");
    }
}
