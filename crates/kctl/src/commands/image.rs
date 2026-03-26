use crate::client::{self, node_proto};
use crate::config::ConnectionInfo;
use anyhow::Result;

pub async fn pull(info: &ConnectionInfo, uri: &str, sha256: Option<&str>) -> Result<()> {
    let sha = sha256.unwrap_or_default();
    if sha.is_empty() {
        anyhow::bail!("--sha256 is required for image pull (integrity check)");
    }

    let file_name = uri.rsplit('/').next().unwrap_or("image.raw");
    let dest = format!(
        "/var/lib/kcore/images/{}-{}",
        &sha[..sha.len().min(12)],
        file_name
    );

    println!("Pulling image from {uri}...");

    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .ensure_image(node_proto::EnsureImageRequest {
            image_url: uri.to_string(),
            image_sha256: sha.to_string(),
            destination_path: dest,
        })
        .await?
        .into_inner();

    if resp.cached {
        println!("Image already cached at {}", resp.path);
    } else if resp.downloaded {
        println!(
            "Image downloaded to {} ({})",
            resp.path,
            client::format_bytes(resp.size_bytes)
        );
    }
    Ok(())
}

pub async fn delete(info: &ConnectionInfo, name: &str, force: bool) -> Result<()> {
    let mut client = client::node_compute_client(info).await?;
    let resp = client
        .delete_image(node_proto::DeleteImageRequest {
            name: name.to_string(),
            force,
        })
        .await?
        .into_inner();

    if resp.success {
        println!("{}", resp.message);
        Ok(())
    } else {
        anyhow::bail!("Delete failed: {}", resp.message);
    }
}
