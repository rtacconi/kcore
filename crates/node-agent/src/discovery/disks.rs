use serde::Deserialize;
use std::process::Command;

use crate::proto::DiskInfo;

#[derive(Deserialize)]
struct LsblkOutput {
    blockdevices: Vec<LsblkDevice>,
}

#[derive(Deserialize)]
struct LsblkDevice {
    name: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    fstype: Option<String>,
    #[serde(default)]
    mountpoint: Option<String>,
    #[serde(rename = "type")]
    device_type: String,
}

pub fn list_disks() -> Result<Vec<DiskInfo>, String> {
    let output = Command::new("lsblk")
        .args(["-J", "-o", "NAME,PATH,SIZE,TYPE,MODEL,FSTYPE,MOUNTPOINT"])
        .output()
        .map_err(|e| format!("running lsblk: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "lsblk failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let parsed: LsblkOutput =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("parsing lsblk JSON: {e}"))?;

    Ok(parsed
        .blockdevices
        .into_iter()
        .filter(|d| d.device_type == "disk")
        .map(|d| DiskInfo {
            name: d.name,
            path: d.path.unwrap_or_default(),
            size: d.size.unwrap_or_default(),
            model: d.model.unwrap_or_default(),
            fstype: d.fstype.unwrap_or_default(),
            mountpoint: d.mountpoint.unwrap_or_default(),
        })
        .collect())
}
