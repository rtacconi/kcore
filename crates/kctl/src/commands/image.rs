use anyhow::Result;
use crate::client::{self, node_proto};
use crate::config::ConnectionInfo;

pub async fn pull(info: &ConnectionInfo, uri: &str) -> Result<()> {
    println!("Pulling image from {uri}...");

    let mut client = client::node_compute_client(info).await?;
    let resp = client
        .pull_image(node_proto::PullImageRequest {
            uri: uri.to_string(),
            name: String::new(),
        })
        .await?
        .into_inner();

    if resp.cached {
        println!("Image already cached at {}", resp.path);
    } else {
        println!(
            "Image downloaded to {} ({})",
            resp.path,
            client::format_bytes(resp.size_bytes)
        );
    }
    Ok(())
}

pub async fn delete(
    info: &ConnectionInfo,
    name: &str,
    force: bool,
) -> Result<()> {
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
    } else {
        eprintln!("Failed: {}", resp.message);
    }
    Ok(())
}
