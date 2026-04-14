use crate::client::{self, controller_proto};
use crate::config::ConnectionInfo;
use crate::output;
use anyhow::Result;

pub async fn list(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .list_volumes(controller_proto::ListVolumesRequest {})
        .await?
        .into_inner();

    if resp.volumes.is_empty() {
        println!("No volumes found.");
    } else {
        output::print_volume_table(&resp.volumes);
    }
    Ok(())
}
