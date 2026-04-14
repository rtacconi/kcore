use anyhow::{Context, Result};

use crate::client::{self, node_proto};
use crate::config::ConnectionInfo;

pub async fn create(
    info: &ConnectionInfo,
    name: &str,
    image: &str,
    network: Option<&str>,
    ports: Vec<String>,
    env: Vec<String>,
    command: Vec<String>,
) -> Result<()> {
    let mut env_map = std::collections::BTreeMap::new();
    for pair in env {
        let trimmed = pair.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (k, v) = trimmed
            .split_once('=')
            .with_context(|| format!("invalid --env value {trimmed:?}; expected KEY=VALUE"))?;
        env_map.insert(k.trim().to_string(), v.to_string());
    }

    let mut client = client::node_container_client(info).await?;
    let resp = client
        .create_container(node_proto::CreateContainerRequest {
            spec: Some(node_proto::ContainerSpec {
                name: name.to_string(),
                image: image.to_string(),
                network: network.unwrap_or_default().to_string(),
                command,
                env: env_map.into_iter().collect(),
                ports,
                storage_backend: String::new(),
                storage_size_bytes: 0,
                mount_target: String::new(),
            }),
        })
        .await?
        .into_inner();

    if let Some(c) = resp.container {
        println!("Container created");
        println!("  Name:   {}", c.name);
        println!("  ID:     {}", c.id);
        println!("  Image:  {}", c.image);
        println!("  Status: {}", c.status);
    } else {
        println!("Container created");
    }
    Ok(())
}

pub async fn create_from_manifest(info: &ConnectionInfo, path: &str) -> Result<()> {
    let data = std::fs::read_to_string(path)?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&data)?;

    let kind = doc["kind"].as_str().unwrap_or("");
    if !kind.eq_ignore_ascii_case("Container") {
        anyhow::bail!("expected kind=Container, got {kind}");
    }

    let name = doc["metadata"]["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("metadata.name is required"))?;
    let spec = &doc["spec"];
    let image = spec["image"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("spec.image is required"))?;
    let network = spec["network"].as_str();
    let ports: Vec<String> = spec["ports"]
        .as_sequence()
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let env: Vec<String> = spec["env"]
        .as_mapping()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| {
                    let key = k.as_str()?;
                    let val = v.as_str().unwrap_or("");
                    Some(format!("{key}={val}"))
                })
                .collect()
        })
        .unwrap_or_default();
    let command: Vec<String> = spec["command"]
        .as_sequence()
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    create(info, name, image, network, ports, env, command).await
}

pub async fn start(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::node_container_client(info).await?;
    let resp = client
        .start_container(node_proto::StartContainerRequest {
            name: name.to_string(),
        })
        .await?
        .into_inner();
    if let Some(c) = resp.container {
        println!("Started {} ({})", c.name, c.status);
    } else {
        println!("Started {name}");
    }
    Ok(())
}

pub async fn stop(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::node_container_client(info).await?;
    let resp = client
        .stop_container(node_proto::StopContainerRequest {
            name: name.to_string(),
        })
        .await?
        .into_inner();
    if let Some(c) = resp.container {
        println!("Stopped {} ({})", c.name, c.status);
    } else {
        println!("Stopped {name}");
    }
    Ok(())
}

pub async fn delete(info: &ConnectionInfo, name: &str, force: bool) -> Result<()> {
    let mut client = client::node_container_client(info).await?;
    let resp = client
        .delete_container(node_proto::DeleteContainerRequest {
            name: name.to_string(),
            force,
        })
        .await?
        .into_inner();
    if resp.success {
        println!("Deleted {name}");
        Ok(())
    } else {
        anyhow::bail!("delete failed for {name}");
    }
}

pub async fn get(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::node_container_client(info).await?;
    let resp = client
        .get_container(node_proto::GetContainerRequest {
            name: name.to_string(),
        })
        .await?
        .into_inner();
    let c = resp.container.context("container not found")?;
    println!("Name:   {}", c.name);
    println!("ID:     {}", c.id);
    println!("Image:  {}", c.image);
    println!("State:  {}", state_label(c.state));
    println!("Status: {}", c.status);
    Ok(())
}

pub async fn list(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::node_container_client(info).await?;
    let resp = client
        .list_containers(node_proto::ListContainersRequest {})
        .await?
        .into_inner();
    if resp.containers.is_empty() {
        println!("No containers found");
        return Ok(());
    }
    println!(
        "{:<24}  {:<16}  {:<28}  {:<16}",
        "NAME", "STATE", "IMAGE", "STATUS"
    );
    for c in resp.containers {
        println!(
            "{:<24}  {:<16}  {:<28}  {:<16}",
            c.name,
            state_label(c.state),
            c.image,
            c.status
        );
    }
    Ok(())
}

fn state_label(state: i32) -> &'static str {
    match node_proto::ContainerState::try_from(state).unwrap_or(node_proto::ContainerState::Unknown)
    {
        node_proto::ContainerState::Created => "created",
        node_proto::ContainerState::Running => "running",
        node_proto::ContainerState::Stopped => "stopped",
        node_proto::ContainerState::Error => "error",
        node_proto::ContainerState::Unknown => "unknown",
    }
}
