use crate::client::{self, controller_proto};
use crate::commands::{container, network, security_group, ssh_key, vm};
use crate::config::ConnectionInfo;
use anyhow::{Context, Result};

pub async fn apply(info: &ConnectionInfo, file: &str, dry_run: bool) -> Result<()> {
    let content = std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?;

    if dry_run {
        println!("--- dry run ---");
        print!("{content}");
        println!("--- end ---");
        return Ok(());
    }

    if let Some(kind) = detect_manifest_kind(&content) {
        match kind.to_ascii_lowercase().as_str() {
            "securitygroup" => return security_group::apply_from_file(info, file).await,
            "vm" => return vm::create_from_manifest(info, file).await,
            "network" => return network::create_from_manifest(info, file).await,
            "sshkey" | "ssh-key" | "ssh_key" => {
                return ssh_key::create_from_manifest(info, file).await
            }
            "container" => return container::create_from_manifest(info, file).await,
            _ => {}
        }
    }

    let mut client = client::controller_admin_client(info).await?;
    let resp = client
        .apply_nix_config(controller_proto::ApplyNixConfigRequest {
            configuration_nix: content,
            rebuild: true,
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

fn detect_manifest_kind(content: &str) -> Option<String> {
    let doc = serde_yaml::from_str::<serde_yaml::Value>(content).ok()?;
    let map = doc.as_mapping()?;
    let key = serde_yaml::Value::String("kind".to_string());
    map.get(&key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::detect_manifest_kind;

    #[test]
    fn detect_manifest_kind_reads_top_level_kind() {
        let manifest = r#"
kind: SecurityGroup
metadata:
  name: expose-nginx-host
"#;
        assert_eq!(
            detect_manifest_kind(manifest).as_deref(),
            Some("SecurityGroup")
        );
    }

    #[test]
    fn detect_manifest_kind_returns_none_without_kind() {
        let manifest = r#"
metadata:
  name: no-kind
"#;
        assert_eq!(detect_manifest_kind(manifest), None);
    }

    #[test]
    fn detect_manifest_kind_vm() {
        let manifest = "kind: VM\nmetadata:\n  name: test\n";
        assert_eq!(detect_manifest_kind(manifest).as_deref(), Some("VM"));
    }

    #[test]
    fn detect_manifest_kind_network() {
        let manifest = "kind: Network\nmetadata:\n  name: net1\n";
        assert_eq!(detect_manifest_kind(manifest).as_deref(), Some("Network"));
    }

    #[test]
    fn detect_manifest_kind_sshkey() {
        let manifest = "kind: SshKey\nmetadata:\n  name: k1\n";
        assert_eq!(detect_manifest_kind(manifest).as_deref(), Some("SshKey"));
    }

    #[test]
    fn detect_manifest_kind_container() {
        let manifest = "kind: Container\nmetadata:\n  name: c1\n";
        assert_eq!(detect_manifest_kind(manifest).as_deref(), Some("Container"));
    }
}
