use anyhow::{Context, Result};
use crate::client::{self, controller_proto};
use crate::config::ConnectionInfo;

pub async fn apply(
    info: &ConnectionInfo,
    file: &str,
    dry_run: bool,
) -> Result<()> {
    let content = std::fs::read_to_string(file)
        .with_context(|| format!("reading {file}"))?;

    if dry_run {
        println!("--- dry run ---");
        print!("{content}");
        println!("--- end ---");
        return Ok(());
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
    } else {
        eprintln!("Failed: {}", resp.message);
    }
    Ok(())
}
