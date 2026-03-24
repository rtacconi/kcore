use std::path::Path;

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
) -> Result<(), Box<dyn std::error::Error>> {
    let (vm_name, vm_cpu, mem_bytes, nics) = if let Some(path) = &args.filename {
        let manifest = parse_vm_manifest(path)?;
        let n = args.name.unwrap_or(manifest.name);
        (n, manifest.cpu, manifest.memory_bytes, manifest.nics)
    } else {
        let n = args
            .name
            .ok_or("NAME required (or use -f to create from a manifest)")?;
        let mem = client::parse_size_bytes(&args.memory)?;
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
) -> Result<(), Box<dyn std::error::Error>> {
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
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = client::controller_client(info).await?;
    client
        .start_vm(proto::StartVmRequest {
            vm_id: vm_id.to_string(),
            target_node: target_node.unwrap_or_default(),
        })
        .await?;
    println!("VM '{vm_id}' desired state set to running (declarative apply started)");
    Ok(())
}

pub async fn stop(
    info: &ConnectionInfo,
    vm_id: &str,
    target_node: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = client::controller_client(info).await?;
    client
        .stop_vm(proto::StopVmRequest {
            vm_id: vm_id.to_string(),
            force: false,
            target_node: target_node.unwrap_or_default(),
        })
        .await?;
    println!("VM '{vm_id}' desired state set to stopped (declarative apply started)");
    Ok(())
}

pub async fn get(
    info: &ConnectionInfo,
    vm_id: &str,
    target_node: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .get_vm(proto::GetVmRequest {
            vm_id: vm_id.to_string(),
            target_node: target_node.unwrap_or_default(),
        })
        .await?
        .into_inner();

    let spec = resp.spec.as_ref().ok_or("no spec in response")?;
    let status = resp.status.as_ref().ok_or("no status in response")?;
    output::print_vm_detail(spec, status, &resp.node_id);

    Ok(())
}

pub async fn list(
    info: &ConnectionInfo,
    target_node: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
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

fn parse_vm_manifest(path: &str) -> Result<VmManifest, Box<dyn std::error::Error>> {
    let data = std::fs::read_to_string(Path::new(path))?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&data)?;

    let kind = doc["kind"].as_str().unwrap_or("");
    if kind != "VM" {
        return Err(format!("expected kind=VM, got {kind}").into());
    }

    let name = doc["metadata"]["name"]
        .as_str()
        .unwrap_or("unnamed")
        .to_string();

    let cpu = doc["spec"]["cpu"].as_i64().unwrap_or(2) as i32;

    let mem_str = doc["spec"]["memoryBytes"].as_str().unwrap_or("2G");
    let memory_bytes = client::parse_size_bytes(mem_str)?;

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
