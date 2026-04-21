use crate::client::{self, controller_proto};
use crate::commands::{container, disk_layout, network, security_group, ssh_key, vm};
use crate::config::ConnectionInfo;
use anyhow::{Context, Result};

/// Manifest kinds that `kctl` handles entirely on the client side, without
/// reaching the controller. Cluster/NodeInstall manifests are bootstrap
/// resources — they configure how to *find* a controller and therefore can't
/// require one to be reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalManifestKind {
    Cluster,
    NodeInstall,
}

/// Outcome of reading and classifying a manifest file once. The whole point
/// of this struct is so callers (e.g. `main.rs`) can avoid reading and YAML-
/// parsing the same file two or three times in a row.
#[derive(Debug, Clone)]
pub struct ClassifiedManifest {
    /// Raw file content, ready to forward to the controller or print as-is.
    pub content: String,
    /// `kind` field value as written in the manifest (case-preserved), or
    /// `None` when the manifest has no `kind:` key.
    pub kind: Option<String>,
    /// `Some` iff `kind` matches a kind handled locally by `kctl`.
    pub local: Option<LocalManifestKind>,
}

/// Read `file`, parse the YAML, and figure out whether the manifest belongs
/// to the controller or to one of the local bootstrap handlers.
///
/// Errors are propagated for both I/O failures (file missing / unreadable)
/// and YAML parse failures, so users see a real diagnostic instead of a
/// silent fallback to the controller path that would then fail with a far
/// less useful message.
pub fn classify_manifest(file: &str) -> Result<ClassifiedManifest> {
    let content = std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?;
    let kind = detect_manifest_kind(&content)
        .with_context(|| format!("parsing manifest YAML in {file}"))?;
    let local = kind.as_deref().and_then(local_manifest_kind);
    Ok(ClassifiedManifest {
        content,
        kind,
        local,
    })
}

/// Returns `Ok(true)` if the manifest at `file` has a `kind` that is handled
/// locally by `kctl` (i.e. without contacting the controller). I/O errors and
/// YAML parse errors are propagated rather than collapsed to `false`, so
/// callers can surface real manifest problems instead of silently falling
/// back to the controller path.
pub fn is_local_manifest_kind(file: &str) -> Result<bool> {
    Ok(classify_manifest(file)?.local.is_some())
}

fn local_manifest_kind(kind: &str) -> Option<LocalManifestKind> {
    match kind.to_ascii_lowercase().as_str() {
        "cluster" => Some(LocalManifestKind::Cluster),
        "nodeinstall" | "node-install" | "node_install" => Some(LocalManifestKind::NodeInstall),
        _ => None,
    }
}

pub async fn apply(info: &ConnectionInfo, file: &str, dry_run: bool) -> Result<()> {
    let classified = classify_manifest(file)?;

    if let Some(local) = classified.local {
        anyhow::bail!(
            "internal error: manifest kind {local:?} must be handled locally; do not route through apply()"
        );
    }

    if dry_run {
        println!("--- dry run ---");
        print!("{}", classified.content);
        println!("--- end ---");
        return Ok(());
    }

    if let Some(kind) = classified.kind.as_deref() {
        match kind.to_ascii_lowercase().as_str() {
            "securitygroup" => return security_group::apply_from_file(info, file).await,
            "vm" => return vm::create_from_manifest(info, file).await,
            "network" => return network::create_from_manifest(info, file).await,
            "sshkey" | "ssh-key" | "ssh_key" => {
                return ssh_key::create_from_manifest(info, file).await
            }
            "container" => return container::create_from_manifest(info, file).await,
            "disklayout" | "disk-layout" | "disk_layout" => {
                return disk_layout::apply_from_file(info, file).await
            }
            _ => {}
        }
    }

    let mut client = client::controller_admin_client(info).await?;
    let resp = client
        .apply_nix_config(controller_proto::ApplyNixConfigRequest {
            configuration_nix: classified.content,
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

/// Parse `kind:` out of a YAML manifest.
///
/// Returns:
/// - `Ok(Some(kind))` when a non-empty top-level `kind:` is present.
/// - `Ok(None)` when the YAML parses cleanly but has no `kind:` key (or it
///   is empty / not a string).
/// - `Err(_)` when the YAML itself fails to parse — propagated so callers
///   surface the syntax error instead of silently treating the manifest as
///   "kind-less".
pub fn detect_manifest_kind(content: &str) -> Result<Option<String>> {
    let doc: serde_yaml::Value =
        serde_yaml::from_str(content).context("invalid YAML in manifest")?;
    let Some(map) = doc.as_mapping() else {
        return Ok(None);
    };
    let key = serde_yaml::Value::String("kind".to_string());
    Ok(map
        .get(&key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unwrap_kind(content: &str) -> Option<String> {
        detect_manifest_kind(content).expect("YAML should parse")
    }

    #[test]
    fn detect_manifest_kind_reads_top_level_kind() {
        let manifest = r#"
kind: SecurityGroup
metadata:
  name: expose-nginx-host
"#;
        assert_eq!(unwrap_kind(manifest).as_deref(), Some("SecurityGroup"));
    }

    #[test]
    fn detect_manifest_kind_returns_none_without_kind() {
        let manifest = r#"
metadata:
  name: no-kind
"#;
        assert_eq!(unwrap_kind(manifest), None);
    }

    #[test]
    fn detect_manifest_kind_vm() {
        let manifest = "kind: VM\nmetadata:\n  name: test\n";
        assert_eq!(unwrap_kind(manifest).as_deref(), Some("VM"));
    }

    #[test]
    fn detect_manifest_kind_network() {
        let manifest = "kind: Network\nmetadata:\n  name: net1\n";
        assert_eq!(unwrap_kind(manifest).as_deref(), Some("Network"));
    }

    #[test]
    fn detect_manifest_kind_sshkey() {
        let manifest = "kind: SshKey\nmetadata:\n  name: k1\n";
        assert_eq!(unwrap_kind(manifest).as_deref(), Some("SshKey"));
    }

    #[test]
    fn detect_manifest_kind_container() {
        let manifest = "kind: Container\nmetadata:\n  name: c1\n";
        assert_eq!(unwrap_kind(manifest).as_deref(), Some("Container"));
    }

    #[test]
    fn detect_manifest_kind_cluster() {
        let manifest =
            "kind: Cluster\nmetadata:\n  name: prod\nspec:\n  controller: 1.2.3.4:9090\n";
        assert_eq!(unwrap_kind(manifest).as_deref(), Some("Cluster"));
    }

    #[test]
    fn detect_manifest_kind_nodeinstall() {
        let manifest = "kind: NodeInstall\nmetadata:\n  name: node1\nspec:\n  node: 1.2.3.4:9091\n  osDisk: /dev/sda\n";
        assert_eq!(unwrap_kind(manifest).as_deref(), Some("NodeInstall"));
    }

    #[test]
    fn detect_manifest_kind_propagates_yaml_errors() {
        // Unterminated YAML mapping: the parser must reject this and we must
        // surface the error instead of silently returning Ok(None).
        let manifest = "kind: VM\n  bad: : :";
        let err = detect_manifest_kind(manifest).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("invalid YAML in manifest"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn is_local_manifest_kind_cluster() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cluster.yaml");
        std::fs::write(
            &path,
            "kind: Cluster\nmetadata:\n  name: test\nspec:\n  controller: 1.2.3.4:9090\n",
        )
        .unwrap();
        assert!(is_local_manifest_kind(path.to_str().unwrap()).unwrap());
    }

    #[test]
    fn is_local_manifest_kind_vm_is_false() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vm.yaml");
        std::fs::write(&path, "kind: VM\nmetadata:\n  name: test\n").unwrap();
        assert!(!is_local_manifest_kind(path.to_str().unwrap()).unwrap());
    }

    #[test]
    fn is_local_manifest_kind_missing_file_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.yaml");
        let err = is_local_manifest_kind(path.to_str().unwrap()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("reading"), "unexpected error: {msg}");
    }

    #[test]
    fn classify_manifest_marks_cluster_as_local() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cluster.yaml");
        std::fs::write(
            &path,
            "kind: Cluster\nmetadata:\n  name: test\nspec:\n  controller: 1.2.3.4:9090\n",
        )
        .unwrap();
        let classified = classify_manifest(path.to_str().unwrap()).unwrap();
        assert_eq!(classified.kind.as_deref(), Some("Cluster"));
        assert_eq!(classified.local, Some(LocalManifestKind::Cluster));
        assert!(classified.content.contains("Cluster"));
    }

    #[test]
    fn classify_manifest_marks_vm_as_remote() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vm.yaml");
        std::fs::write(&path, "kind: VM\nmetadata:\n  name: test\n").unwrap();
        let classified = classify_manifest(path.to_str().unwrap()).unwrap();
        assert_eq!(classified.kind.as_deref(), Some("VM"));
        assert_eq!(classified.local, None);
    }

    #[test]
    fn classify_manifest_propagates_yaml_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.yaml");
        std::fs::write(&path, "kind: VM\n  bad: : :").unwrap();
        let err = classify_manifest(path.to_str().unwrap()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("parsing manifest"), "unexpected error: {msg}");
    }
}

/// Property-based tests (Phase 2) — manifest classification.
#[cfg(test)]
mod proptests {
    use super::{detect_manifest_kind, local_manifest_kind, LocalManifestKind};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// `local_manifest_kind` is **case-insensitive** on its input
        /// and recognises all three documented `nodeinstall` aliases.
        #[test]
        fn local_manifest_kind_aliases(
            base in prop::sample::select(vec![
                "cluster", "nodeinstall", "node-install", "node_install",
            ]),
            uppercase in any::<bool>(),
        ) {
            let s = if uppercase { base.to_uppercase() } else { base.to_string() };
            let expected = match base {
                "cluster" => Some(LocalManifestKind::Cluster),
                _ => Some(LocalManifestKind::NodeInstall),
            };
            prop_assert_eq!(local_manifest_kind(&s), expected);
        }

        /// Anything outside the alias set returns `None`.
        #[test]
        fn local_manifest_kind_none_for_unknown(s in "[a-zA-Z0-9_-]{1,16}") {
            let lower = s.to_ascii_lowercase();
            if !matches!(lower.as_str(), "cluster" | "nodeinstall" | "node-install" | "node_install") {
                prop_assert_eq!(local_manifest_kind(&s), None);
            }
        }

        /// `detect_manifest_kind` returns `Ok(Some(kind))` when given a
        /// well-formed YAML map with a `kind:` string entry.
        #[test]
        fn detect_manifest_kind_extracts_top_level_key(
            kind in "[a-zA-Z][a-zA-Z0-9]{0,16}",
            name in "[a-z][a-z0-9-]{0,16}",
        ) {
            let yaml = format!("kind: {kind}\nmetadata:\n  name: {name}\n");
            let got = detect_manifest_kind(&yaml).expect("YAML parses");
            prop_assert_eq!(got, Some(kind));
        }

        /// Empty `kind:` is treated as missing (`Ok(None)`).
        #[test]
        fn detect_manifest_kind_empty_kind_is_none(_seed in any::<u8>()) {
            let yaml = "kind: \"\"\nmetadata:\n  name: x\n";
            prop_assert_eq!(detect_manifest_kind(yaml).expect("parses"), None);
        }
    }
}
