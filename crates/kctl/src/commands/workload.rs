use anyhow::{Context, Result};

use crate::client::{self, controller_proto};
use crate::config::ConnectionInfo;

pub async fn create(
    info: &ConnectionInfo,
    kind: &str,
    name: &str,
    image: Option<&str>,
    cpu: i32,
    memory_bytes: i64,
    network: Option<&str>,
    target_node: Option<&str>,
    storage_backend: i32,
    storage_size_bytes: i64,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let kind = normalize_kind(kind)?;
    let req = match kind {
        controller_proto::WorkloadKind::Vm => controller_proto::CreateWorkloadRequest {
            kind: controller_proto::WorkloadKind::Vm as i32,
            target_node: target_node.unwrap_or_default().to_string(),
            vm_spec: Some(controller_proto::VmSpec {
                id: String::new(),
                name: name.to_string(),
                cpu,
                memory_bytes,
                disks: Vec::new(),
                nics: vec![controller_proto::Nic {
                    network: network.unwrap_or("default").to_string(),
                    model: "virtio".to_string(),
                    mac_address: String::new(),
                }],
                storage_backend: String::new(),
                storage_size_bytes: 0,
            }),
            container_spec: None,
            image_url: image.unwrap_or_default().to_string(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: String::new(),
            image_format: String::new(),
            ssh_key_names: Vec::new(),
            storage_backend,
            storage_size_bytes,
        },
        controller_proto::WorkloadKind::Container => controller_proto::CreateWorkloadRequest {
            kind: controller_proto::WorkloadKind::Container as i32,
            target_node: target_node.unwrap_or_default().to_string(),
            vm_spec: None,
            container_spec: Some(controller_proto::ContainerSpec {
                name: name.to_string(),
                image: image.unwrap_or("nginx:alpine").to_string(),
                network: network.unwrap_or_default().to_string(),
                command: Vec::new(),
                env: std::collections::HashMap::new(),
                ports: Vec::new(),
                storage_backend: String::new(),
                storage_size_bytes: 0,
                mount_target: String::new(),
            }),
            image_url: String::new(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: String::new(),
            image_format: String::new(),
            ssh_key_names: Vec::new(),
            storage_backend,
            storage_size_bytes,
        },
        controller_proto::WorkloadKind::Unspecified => unreachable!(),
    };
    let resp = client.create_workload(req).await?.into_inner();
    println!(
        "Created workload {} on node {} ({})",
        resp.workload_id,
        resp.node_id,
        match controller_proto::WorkloadKind::try_from(resp.kind)
            .unwrap_or(controller_proto::WorkloadKind::Unspecified)
        {
            controller_proto::WorkloadKind::Vm => "vm",
            controller_proto::WorkloadKind::Container => "container",
            controller_proto::WorkloadKind::Unspecified => "unknown",
        }
    );
    Ok(())
}

pub async fn list(info: &ConnectionInfo, kind: &str, target_node: Option<&str>) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let kind = normalize_kind_or_unspecified(kind)?;
    let resp = client
        .list_workloads(controller_proto::ListWorkloadsRequest {
            kind: kind as i32,
            target_node: target_node.unwrap_or_default().to_string(),
        })
        .await?
        .into_inner();
    let has_vms = !resp.vms.is_empty();
    let has_containers = !resp.containers.is_empty();
    if has_vms {
        println!("VM workloads:");
        for vm in &resp.vms {
            println!("  {} ({})", vm.name, vm.id);
        }
    }
    if has_containers {
        println!("Container workloads:");
        for c in &resp.containers {
            println!("  {} ({}) {}", c.name, c.id, c.status);
        }
    }
    if !has_vms && !has_containers {
        println!("No workloads found");
    }
    Ok(())
}

pub async fn get(
    info: &ConnectionInfo,
    kind: &str,
    id: &str,
    target_node: Option<&str>,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let kind = normalize_kind(kind)?;
    let resp = client
        .get_workload(controller_proto::GetWorkloadRequest {
            kind: kind as i32,
            workload_id: id.to_string(),
            target_node: target_node.unwrap_or_default().to_string(),
        })
        .await?
        .into_inner();
    println!(
        "Workload kind: {:?}",
        controller_proto::WorkloadKind::try_from(resp.kind)
    );
    println!("Node: {}", resp.node_id);
    if let Some(vm) = resp.vm_spec {
        println!("VM {} cpu={} mem={}", vm.name, vm.cpu, vm.memory_bytes);
    }
    if let Some(c) = resp.container_info {
        println!("Container {} image={} status={}", c.name, c.image, c.status);
    }
    Ok(())
}

pub async fn delete(
    info: &ConnectionInfo,
    kind: &str,
    id: &str,
    target_node: Option<&str>,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let kind = normalize_kind(kind)?;
    client
        .delete_workload(controller_proto::DeleteWorkloadRequest {
            kind: kind as i32,
            workload_id: id.to_string(),
            target_node: target_node.unwrap_or_default().to_string(),
        })
        .await?;
    println!("Deleted workload {id}");
    Ok(())
}

pub async fn set_state(
    info: &ConnectionInfo,
    kind: &str,
    id: &str,
    running: bool,
    target_node: Option<&str>,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let kind = normalize_kind(kind)?;
    let desired = if running {
        controller_proto::WorkloadDesiredState::Running as i32
    } else {
        controller_proto::WorkloadDesiredState::Stopped as i32
    };
    client
        .set_workload_desired_state(controller_proto::SetWorkloadDesiredStateRequest {
            kind: kind as i32,
            workload_id: id.to_string(),
            desired_state: desired,
            target_node: target_node.unwrap_or_default().to_string(),
        })
        .await?;
    println!(
        "Set workload {id} to {}",
        if running { "running" } else { "stopped" }
    );
    Ok(())
}

fn normalize_kind(kind: &str) -> Result<controller_proto::WorkloadKind> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "vm" => Ok(controller_proto::WorkloadKind::Vm),
        "container" => Ok(controller_proto::WorkloadKind::Container),
        other => Err(anyhow::anyhow!(
            "invalid kind '{other}', expected vm|container"
        )),
    }
}

fn normalize_kind_or_unspecified(kind: &str) -> Result<controller_proto::WorkloadKind> {
    let trimmed = kind.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("all") {
        return Ok(controller_proto::WorkloadKind::Unspecified);
    }
    normalize_kind(trimmed).with_context(|| "invalid --kind value")
}
