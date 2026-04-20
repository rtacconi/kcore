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

pub async fn create_from_manifest(info: &ConnectionInfo, path: &str) -> Result<()> {
    let args = CreateArgs {
        name: None,
        filename: Some(path.to_string()),
        cpu: 2,
        memory: "2G".to_string(),
        image: None,
        image_sha256: None,
        image_path: None,
        image_format: None,
        network: None,
        target_node: None,
        wait: false,
        wait_for_ssh: false,
        wait_timeout_seconds: 300,
        ssh_port: 22,
        ssh_probe_timeout_ms: 1200,
        ssh_keys: vec![],
        ssh_public_keys: vec![],
        cloud_init_user_data_file: None,
        username: None,
        password: None,
        compliant: true,
        storage_backend: None,
        storage_size_bytes: None,
        target_dc: None,
    };
    create(info, args).await
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
        manifest_target_node,
        manifest_target_dc,
        manifest_ssh_keys,
        manifest_cloud_init,
        manifest_desired_state,
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
            manifest.target_node,
            manifest.target_dc,
            manifest.ssh_keys,
            manifest.cloud_init_user_data,
            manifest.desired_state,
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
        (
            n,
            cpu,
            mem,
            nics,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            vec![],
            None,
            proto::VmDesiredState::Unspecified,
        )
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
        storage_backend: String::new(),
        storage_size_bytes: 0,
        desired_state: manifest_desired_state as i32,
    };

    let target_node = args
        .target_node
        .or(manifest_target_node)
        .unwrap_or_default();
    let target_dc = args.target_dc.or(manifest_target_dc).unwrap_or_default();
    let ssh_key_names = if args.ssh_keys.is_empty() {
        manifest_ssh_keys
    } else {
        args.ssh_keys
    };
    let cloud_init_user_data = if cloud_init_user_data.is_empty() {
        manifest_cloud_init.unwrap_or_default()
    } else {
        cloud_init_user_data
    };

    let req = proto::CreateVmRequest {
        target_node,
        spec: Some(spec),
        image_url: image.url,
        image_sha256: image.sha256,
        cloud_init_user_data,
        image_path: image.path,
        image_format: image.format,
        ssh_key_names,
        storage_backend: storage_backend_to_proto(&storage_backend),
        storage_size_bytes,
        target_dc,
    };

    let resp = client.create_vm(req).await?.into_inner();

    let label = format!("VM '{vm_name}'");
    println!(
        "{}",
        crate::apply_summary::render_apply_summary(resp.action, &resp.changed_fields, &label)
    );
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
    target_node: Option<String>,
    target_dc: Option<String>,
    ssh_keys: Vec<String>,
    cloud_init_user_data: Option<String>,
    desired_state: proto::VmDesiredState,
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

    let cpu_raw = doc["spec"]["cpu"].as_i64().unwrap_or(2);
    if cpu_raw <= 0 {
        bail!("spec.cpu must be > 0 (got {cpu_raw})");
    }
    if cpu_raw > i32::MAX as i64 {
        bail!("spec.cpu is too large: {cpu_raw}");
    }
    let cpu = cpu_raw as i32;

    // Accept memoryBytes as either a human-readable string ("2G", "512M")
    // OR a raw integer count of bytes — Kubernetes-style manifests are
    // commonly checked into source control as plain integers, and the
    // previous `as_str().unwrap_or("2G")` silently dropped integer values
    // and used the 2G default, sizing every VM at 2G regardless of intent.
    let mem_node = &doc["spec"]["memoryBytes"];
    let memory_bytes = if let Some(n) = mem_node.as_i64() {
        n
    } else if let Some(n) = mem_node.as_u64() {
        i64::try_from(n).map_err(|_| anyhow::anyhow!("spec.memoryBytes is too large: {n}"))?
    } else if let Some(s) = mem_node.as_str() {
        client::parse_size_bytes(s).map_err(|e| anyhow::anyhow!(e))?
    } else if mem_node.is_null() {
        client::parse_size_bytes("2G").map_err(|e| anyhow::anyhow!(e))?
    } else {
        bail!("spec.memoryBytes must be an integer or a size string (e.g. \"2G\")");
    };
    if memory_bytes <= 0 {
        bail!("spec.memoryBytes must be > 0 (got {memory_bytes})");
    }

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

    let target_node = doc["spec"]["targetNode"]
        .as_str()
        .or_else(|| doc["spec"]["target_node"].as_str())
        .map(|s| s.to_string());
    let target_dc = doc["spec"]["dc"]
        .as_str()
        .or_else(|| doc["spec"]["targetDc"].as_str())
        .or_else(|| doc["spec"]["target_dc"].as_str())
        .map(|s| s.to_string());
    let ssh_keys = doc["spec"]["sshKeys"]
        .as_sequence()
        .or_else(|| doc["spec"]["ssh_keys"].as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let cloud_init_user_data = doc["spec"]["cloudInitUserData"]
        .as_str()
        .or_else(|| doc["spec"]["cloud_init_user_data"].as_str())
        .map(|s| s.to_string());

    let desired_state_raw = doc["spec"]["desiredState"]
        .as_str()
        .or_else(|| doc["spec"]["desired_state"].as_str())
        .map(|s| s.trim().to_ascii_lowercase());
    let desired_state = match desired_state_raw.as_deref() {
        Some("running") | Some("run") | Some("started") => proto::VmDesiredState::Running,
        Some("stopped") | Some("stop") | Some("halted") => proto::VmDesiredState::Stopped,
        Some(other) => {
            bail!("invalid spec.desiredState '{other}': expected 'running' or 'stopped'")
        }
        None => proto::VmDesiredState::Unspecified,
    };

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
        target_node,
        target_dc,
        ssh_keys,
        cloud_init_user_data,
        desired_state,
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

    #[test]
    fn parse_vm_manifest_parses_all_fields() {
        let path = std::env::temp_dir().join("kctl-test-vm-manifest.yaml");
        std::fs::write(
            &path,
            r#"
kind: VM
metadata:
  name: full-test
spec:
  cpu: 4
  memoryBytes: "8G"
  storageBackend: zfs
  storageSizeBytes: "20G"
  targetNode: node-42
  dc: eu-west
  sshKeys:
    - deploy-key
    - ops-key
  cloudInitUserData: |
    #cloud-config
    packages: [htop]
  nics:
    - network: vxlan-prod
    - network: nat-mgmt
      model: e1000
  disks:
    - image: https://example.com/debian.qcow2
      sha256: abc123
      format: qcow2
"#,
        )
        .unwrap();
        let m = parse_vm_manifest(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(m.name, "full-test");
        assert_eq!(m.cpu, 4);
        assert_eq!(m.memory_bytes, 8_589_934_592);
        assert_eq!(m.storage_backend.as_deref(), Some("zfs"));
        assert_eq!(m.storage_size_bytes, Some(21_474_836_480));
        assert_eq!(m.target_node.as_deref(), Some("node-42"));
        assert_eq!(m.target_dc.as_deref(), Some("eu-west"));
        assert_eq!(m.ssh_keys, vec!["deploy-key", "ops-key"]);
        assert!(m.cloud_init_user_data.as_ref().unwrap().contains("htop"));
        assert_eq!(m.nics.len(), 2);
        assert_eq!(m.nics[0].network, "vxlan-prod");
        assert_eq!(m.nics[1].network, "nat-mgmt");
        assert_eq!(m.nics[1].model, "e1000");
        assert_eq!(m.image.as_deref(), Some("https://example.com/debian.qcow2"));
        assert_eq!(m.image_sha256.as_deref(), Some("abc123"));
        assert_eq!(m.image_format.as_deref(), Some("qcow2"));
        assert_eq!(m.desired_state, proto::VmDesiredState::Unspecified);
    }

    fn write_manifest_with_desired_state(value: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("kctl-test-vm-ds-{value}.yaml"));
        std::fs::write(
            &path,
            format!(
                r#"
kind: VM
metadata:
  name: ds-test
spec:
  cpu: 1
  memoryBytes: "512M"
  desiredState: {value}
  nics:
    - network: default
"#
            ),
        )
        .unwrap();
        path
    }

    #[test]
    fn parse_vm_manifest_desired_state_running_aliases() {
        for alias in ["running", "run", "started", "Running", "STARTED"] {
            let path = write_manifest_with_desired_state(alias);
            let m = parse_vm_manifest(path.to_str().unwrap()).unwrap();
            let _ = std::fs::remove_file(&path);
            assert_eq!(
                m.desired_state,
                proto::VmDesiredState::Running,
                "alias {alias} should map to Running"
            );
        }
    }

    #[test]
    fn parse_vm_manifest_desired_state_stopped_aliases() {
        for alias in ["stopped", "stop", "halted", "Stopped"] {
            let path = write_manifest_with_desired_state(alias);
            let m = parse_vm_manifest(path.to_str().unwrap()).unwrap();
            let _ = std::fs::remove_file(&path);
            assert_eq!(
                m.desired_state,
                proto::VmDesiredState::Stopped,
                "alias {alias} should map to Stopped"
            );
        }
    }

    #[test]
    fn parse_vm_manifest_desired_state_invalid_rejected() {
        let path = write_manifest_with_desired_state("paused");
        let result = parse_vm_manifest(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected 'paused' to be rejected"),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("invalid spec.desiredState"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn parse_vm_manifest_accepts_memory_bytes_as_integer() {
        // Regression: `as_str().unwrap_or("2G")` used to silently drop a
        // numeric memoryBytes and size every VM at 2G, with no warning.
        let path = std::env::temp_dir().join("kctl-test-vm-mem-int.yaml");
        std::fs::write(
            &path,
            r#"
kind: VM
metadata:
  name: int-mem
spec:
  cpu: 2
  memoryBytes: 8589934592
  nics:
    - network: default
"#,
        )
        .unwrap();
        let m = parse_vm_manifest(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            m.memory_bytes, 8_589_934_592,
            "8 GiB integer must round-trip"
        );
    }

    #[test]
    fn parse_vm_manifest_rejects_zero_cpu() {
        let path = std::env::temp_dir().join("kctl-test-vm-cpu-zero.yaml");
        std::fs::write(
            &path,
            r#"
kind: VM
metadata:
  name: bad
spec:
  cpu: 0
  memoryBytes: "1G"
"#,
        )
        .unwrap();
        let result = parse_vm_manifest(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("cpu: 0 must be rejected client-side"),
        };
        assert!(
            err.to_string().contains("spec.cpu must be > 0"),
            "got: {err}"
        );
    }

    #[test]
    fn parse_vm_manifest_rejects_zero_memory() {
        let path = std::env::temp_dir().join("kctl-test-vm-mem-zero.yaml");
        std::fs::write(
            &path,
            r#"
kind: VM
metadata:
  name: bad
spec:
  cpu: 1
  memoryBytes: 0
"#,
        )
        .unwrap();
        let result = parse_vm_manifest(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("memoryBytes: 0 must be rejected client-side"),
        };
        assert!(
            err.to_string().contains("spec.memoryBytes must be > 0"),
            "got: {err}"
        );
    }

    #[test]
    fn parse_vm_manifest_rejects_non_scalar_memory() {
        let path = std::env::temp_dir().join("kctl-test-vm-mem-bad.yaml");
        std::fs::write(
            &path,
            r#"
kind: VM
metadata:
  name: bad
spec:
  cpu: 1
  memoryBytes:
    nested: 1
"#,
        )
        .unwrap();
        let result = parse_vm_manifest(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert!(
            result.is_err(),
            "mapping memoryBytes must be rejected, not silently coerced"
        );
    }

    #[test]
    fn parse_vm_manifest_desired_state_snake_case_accepted() {
        let path = std::env::temp_dir().join("kctl-test-vm-ds-snake.yaml");
        std::fs::write(
            &path,
            r#"
kind: VM
metadata:
  name: ds-snake
spec:
  cpu: 1
  memoryBytes: "256M"
  desired_state: stopped
"#,
        )
        .unwrap();
        let m = parse_vm_manifest(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(m.desired_state, proto::VmDesiredState::Stopped);
    }
}

/// Property-based tests (Phase 2) — VM CLI helpers.
#[cfg(test)]
mod proptests {
    use super::{
        infer_format_from_path, normalize_image_format, normalize_storage_backend_arg,
        select_image_mode, storage_backend_to_proto, yaml_escape_double_quoted,
    };
    use crate::client::controller_proto as proto;
    use proptest::prelude::*;

    /// Local re-implementation of the YAML safety predicate: any
    /// double-quote and any backslash must be escaped, and there must
    /// be no dangling backslash at end of string.
    fn is_safely_yaml_double_quoted(s: &str) -> bool {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => {
                    if i + 1 >= bytes.len() {
                        return false;
                    }
                    i += 2;
                }
                b'"' => return false,
                _ => i += 1,
            }
        }
        true
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// `yaml_escape_double_quoted` must produce a string that is
        /// safe to embed inside a YAML double-quoted scalar.
        #[test]
        fn yaml_escape_double_quoted_is_safe(s in ".{0,64}") {
            let escaped = yaml_escape_double_quoted(&s);
            prop_assert!(
                is_safely_yaml_double_quoted(&escaped),
                "{escaped:?} is not YAML-safe"
            );
        }

        /// Strings without `\` or `"` are returned unchanged.
        #[test]
        fn yaml_escape_identity_on_safe_input(s in "[a-zA-Z0-9 _\\-./:]{0,32}") {
            prop_assert_eq!(yaml_escape_double_quoted(&s), s);
        }

        /// `infer_format_from_path` is case-insensitive on the suffix
        /// and only ever returns `raw` or `qcow2`.
        #[test]
        fn infer_format_from_path_known_set(s in ".{0,64}") {
            let f = infer_format_from_path(&s);
            prop_assert!(f == "raw" || f == "qcow2");
        }

        /// `infer_format_from_path` returns `qcow2` iff the lowercased
        /// path ends with `.qcow2` or `.qcow`.
        #[test]
        fn infer_format_from_path_matches_predicate(
            stem in "[a-z0-9_-]{1,16}",
            ext in prop::sample::select(vec![".qcow", ".qcow2", ".raw", ".img", ""]),
            uppercase in any::<bool>(),
        ) {
            let mut s = format!("{stem}{ext}");
            if uppercase {
                s = s.to_uppercase();
            }
            let lower = s.to_ascii_lowercase();
            let expected = if lower.ends_with(".qcow2") || lower.ends_with(".qcow") {
                "qcow2"
            } else {
                "raw"
            };
            prop_assert_eq!(infer_format_from_path(&s), expected);
        }

        /// `normalize_image_format` accepts iff trimmed-lowercased value
        /// is `raw` or `qcow2`. Output is idempotent.
        #[test]
        fn normalize_image_format_acceptance(s in ".{0,16}") {
            let normalized = s.trim().to_ascii_lowercase();
            let predicate = normalized == "raw" || normalized == "qcow2";
            let result = normalize_image_format(&s);
            prop_assert_eq!(result.is_ok(), predicate);
            if let Ok(v) = result {
                prop_assert_eq!(normalize_image_format(&v).unwrap(), v);
            }
        }

        /// `normalize_storage_backend_arg` accepts iff trimmed-lowercased
        /// value is in `{filesystem, lvm, zfs}`.
        #[test]
        fn normalize_storage_backend_arg_acceptance(s in ".{0,16}") {
            let normalized = s.trim().to_ascii_lowercase();
            let predicate = matches!(normalized.as_str(), "filesystem" | "lvm" | "zfs");
            prop_assert_eq!(normalize_storage_backend_arg(&s).is_ok(), predicate);
        }

        /// **Round-trip**: `normalize_storage_backend_arg` followed by
        /// `storage_backend_to_proto` always produces a non-`Unspecified`
        /// proto value.
        #[test]
        fn storage_backend_round_trip(s in prop::sample::select(vec![
            "filesystem", "lvm", "zfs", "FILESYSTEM", "LVM", "ZFS", " lvm  ",
        ])) {
            let normalized = normalize_storage_backend_arg(s).expect("known backend");
            let p = storage_backend_to_proto(&normalized);
            prop_assert!(p != proto::StorageBackendType::Unspecified as i32);
        }

        /// `storage_backend_to_proto` returns one of the four known
        /// enum values for any input and never panics.
        #[test]
        fn storage_backend_to_proto_never_panics(s in ".{0,16}") {
            let v = storage_backend_to_proto(&s);
            prop_assert!(
                v == proto::StorageBackendType::Filesystem as i32
                    || v == proto::StorageBackendType::Lvm as i32
                    || v == proto::StorageBackendType::Zfs as i32
                    || v == proto::StorageBackendType::Unspecified as i32
            );
        }

        /// `select_image_mode` rejects when both CLI URL AND CLI path
        /// are set.
        #[test]
        fn select_image_mode_rejects_cli_conflict(
            cli_image in "[a-z]{1,8}",
            cli_path in "[/a-z]{1,8}",
        ) {
            let result = select_image_mode(
                Some(&cli_image),
                Some(&cli_path),
                None,
                None,
            );
            prop_assert!(result.is_err());
        }

        /// `select_image_mode` returns `"url"` for any non-empty CLI
        /// `--image` (alone) regardless of whether it looks like a URL
        /// (the URL-format check happens later in `resolve_create_image_source`).
        #[test]
        fn select_image_mode_cli_image_wins(cli_image in "[a-z]{1,16}") {
            let result = select_image_mode(Some(&cli_image), None, None, None);
            prop_assert_eq!(result.unwrap(), "url");
        }
    }
}
