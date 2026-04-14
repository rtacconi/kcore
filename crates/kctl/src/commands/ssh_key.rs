use anyhow::Result;

use crate::client::{self, controller_proto as proto};
use crate::config::ConnectionInfo;

pub async fn create(info: &ConnectionInfo, name: &str, public_key: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .create_ssh_key(proto::CreateSshKeyRequest {
            name: name.to_string(),
            public_key: public_key.to_string(),
        })
        .await?
        .into_inner();

    if resp.success {
        println!("{}", resp.message);
    }
    Ok(())
}

pub async fn create_from_manifest(info: &ConnectionInfo, path: &str) -> Result<()> {
    let data = std::fs::read_to_string(path)?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&data)?;

    let kind = doc["kind"].as_str().unwrap_or("");
    if !kind.eq_ignore_ascii_case("SshKey") && !kind.eq_ignore_ascii_case("SSHKey") {
        anyhow::bail!("expected kind=SshKey, got {kind}");
    }

    let name = doc["metadata"]["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("metadata.name is required"))?;
    let public_key = doc["spec"]["publicKey"]
        .as_str()
        .or_else(|| doc["spec"]["public_key"].as_str())
        .ok_or_else(|| anyhow::anyhow!("spec.publicKey is required"))?;

    create(info, name, public_key).await
}

pub async fn delete(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    client
        .delete_ssh_key(proto::DeleteSshKeyRequest {
            name: name.to_string(),
        })
        .await?;
    println!("SSH key '{name}' deleted");
    Ok(())
}

pub async fn list(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .list_ssh_keys(proto::ListSshKeysRequest {})
        .await?
        .into_inner();

    if resp.keys.is_empty() {
        println!("No SSH keys found");
        return Ok(());
    }

    println!("{:<20}  {:<60}", "NAME", "PUBLIC KEY (truncated)");
    for k in &resp.keys {
        let truncated = if k.public_key.len() > 55 {
            format!("{}...", &k.public_key[..55])
        } else {
            k.public_key.clone()
        };
        println!("{:<20}  {:<60}", k.name, truncated);
    }
    Ok(())
}

pub async fn get(info: &ConnectionInfo, name: &str) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .get_ssh_key(proto::GetSshKeyRequest {
            name: name.to_string(),
        })
        .await?
        .into_inner();

    if let Some(key) = resp.key {
        println!("Name:       {}", key.name);
        println!("Public Key: {}", key.public_key);
    }
    Ok(())
}
