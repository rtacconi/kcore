use std::path::Path;

use anyhow::{bail, Context, Result};
use crate::client::{self, controller_proto as proto};
use crate::config::ConnectionInfo;
use crate::output;

pub struct CreateArgs {
    pub name: Option<String>,
    pub filename: Option<String>,
    pub cpu: i32,
    pub memory: String,
    pub image: Option<String>,
    pub network: Option<String>,
    pub target_node: Option<String>,
}

pub async fn create(
    info: &ConnectionInfo,
    args: CreateArgs,
) -> Result<()> {
    let (vm_name, vm_cpu, mem_bytes, nics) = if let Some(path) = &args.filename {
        let manifest = parse_vm_manifest(path)?;
        let n = args.name.unwrap_or(manifest.name);
        (n, manifest.cpu, manifest.memory_bytes, manifest.nics)
    } else {
        let n = args
            .name
            .context("NAME required (or use -f to create from a manifest)")?;
        let mem = client::parse_size_bytes(&args.memory)
            .map_err(|e| anyhow::anyhow!(e))?;
        let cpu = args.cpu;
        let nics = args
            .network
            .map(|net| {
                vec![proto::Nic {
                    network: net,
                    model: "virtio".to_string(),
                    mac_address: String::new(),
                }]
            })
            .unwrap_or_default();
        (n, cpu, mem, nics)
    };

    let mut client = client::controller_client(info).await?;

    let spec = proto::VmSpec {
        id: String::new(),
        name: vm_name.clone(),
        cpu: vm_cpu,
        memory_bytes: mem_bytes,
        disks: vec![],
        nics,
    };

    let mut req = proto::CreateVmRequest {
        target_node: args.target_node.unwrap_or_default(),
        spec: Some(spec),
    };

    if let Some(img) = &args.image {
        if let Some(spec) = req.spec.as_mut() {
            spec.disks.push(proto::Disk {
                name: "boot".to_string(),
                backend_handle: img.clone(),
                bus: String::new(),
                device: String::new(),
            });
        }
    }

    let resp = client.create_vm(req).await?.into_inner();

    println!("VM '{vm_name}' created");
    println!("  ID:   {}", resp.vm_id);
    println!("  Node: {}", resp.node_id);
    println!("  CPU:  {vm_cpu} cores");
    println!("  Mem:  {}", client::format_bytes(mem_bytes));

    Ok(())
}

pub async fn delete(
    info: &ConnectionInfo,
    vm_id: &str,
    target_node: Option<String>,
) -> Result<()> {
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

pub async fn start(
    info: &ConnectionInfo,
    vm_id: &str,
    target_node: Option<String>,
) -> Result<()> {
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

pub async fn stop(
    info: &ConnectionInfo,
    vm_id: &str,
    target_node: Option<String>,
) -> Result<()> {
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

pub async fn get(
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

    Ok(())
}

pub async fn list(
    info: &ConnectionInfo,
    target_node: Option<String>,
) -> Result<()> {
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
    let memory_bytes = client::parse_size_bytes(mem_str)
        .map_err(|e| anyhow::anyhow!(e))?;

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

    Ok(VmManifest {
        name,
        cpu,
        memory_bytes,
        nics,
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
}
