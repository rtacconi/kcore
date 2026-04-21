//! lsblk JSON parser feeding [`classifier::LsblkSnapshot`].
//!
//! We invoke `lsblk -J -p -o NAME,PATH,FSTYPE,MOUNTPOINTS,PKNAME,TYPE` and
//! parse the JSON into a flat [`classifier::LsblkSnapshot`]. Recursion over
//! the `children` tree is explicit so we can propagate `parent_path` without
//! relying on lsblk's own PKNAME when it is empty (which happens for nested
//! `dm-` nodes on some kernels).

use serde_json::Value;

use crate::disk::classifier::{BlockDevice, LsblkSnapshot};

pub async fn snapshot() -> std::io::Result<LsblkSnapshot> {
    let out = tokio::process::Command::new("lsblk")
        .args(["-J", "-p", "-o", "NAME,PATH,FSTYPE,MOUNTPOINTS,PKNAME,TYPE"])
        .output()
        .await?;
    if !out.status.success() {
        return Err(std::io::Error::other(format!(
            "lsblk exited with status {}",
            out.status
        )));
    }
    parse(&String::from_utf8_lossy(&out.stdout))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

pub fn parse(json: &str) -> Result<LsblkSnapshot, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| format!("parse lsblk json: {e}"))?;
    let block_devices = root
        .get("blockdevices")
        .and_then(Value::as_array)
        .ok_or_else(|| "lsblk json missing blockdevices array".to_string())?;
    let mut out = Vec::new();
    for dev in block_devices {
        walk(dev, None, &mut out);
    }
    Ok(LsblkSnapshot { devices: out })
}

fn walk(node: &Value, parent_path: Option<&str>, out: &mut Vec<BlockDevice>) {
    let path = node
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if path.is_empty() {
        return;
    }
    let kind = node
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let fstype = node
        .get("fstype")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let mountpoints = node
        .get("mountpoints")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let parent = parent_path.map(str::to_string).or_else(|| {
        node.get("pkname")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });
    out.push(BlockDevice {
        path: path.clone(),
        kind,
        fstype,
        mountpoints,
        parent_path: parent,
    });
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            walk(child, Some(&path), out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_disk() {
        let json = r#"{
            "blockdevices": [
                {"name":"sda","path":"/dev/sda","fstype":null,"mountpoints":[null],"pkname":null,"type":"disk"}
            ]
        }"#;
        let snap = parse(json).expect("parse");
        assert_eq!(snap.devices.len(), 1);
        assert_eq!(snap.devices[0].path, "/dev/sda");
        assert_eq!(snap.devices[0].kind, "disk");
        assert!(snap.devices[0].fstype.is_none());
        assert!(snap.devices[0].mountpoints.is_empty());
    }

    #[test]
    fn parses_nested_partition_with_mountpoint() {
        let json = r#"{
            "blockdevices": [
                {
                    "name":"sda","path":"/dev/sda","fstype":null,"mountpoints":[null],"pkname":null,"type":"disk",
                    "children":[
                        {"name":"sda1","path":"/dev/sda1","fstype":"ext4","mountpoints":["/"],"pkname":"/dev/sda","type":"part"}
                    ]
                }
            ]
        }"#;
        let snap = parse(json).expect("parse");
        assert_eq!(snap.devices.len(), 2);
        let part = &snap.devices[1];
        assert_eq!(part.path, "/dev/sda1");
        assert_eq!(part.parent_path.as_deref(), Some("/dev/sda"));
        assert_eq!(part.fstype.as_deref(), Some("ext4"));
        assert_eq!(part.mountpoints, vec!["/".to_string()]);
    }

    #[test]
    fn propagates_parent_even_when_pkname_missing() {
        let json = r#"{
            "blockdevices": [
                {
                    "name":"nvme0n1","path":"/dev/nvme0n1","fstype":null,"mountpoints":[null],"type":"disk",
                    "children":[
                        {"name":"nvme0n1p1","path":"/dev/nvme0n1p1","fstype":"LVM2_member","mountpoints":[null],"type":"part"}
                    ]
                }
            ]
        }"#;
        let snap = parse(json).expect("parse");
        let part = snap
            .devices
            .iter()
            .find(|d| d.path == "/dev/nvme0n1p1")
            .unwrap();
        assert_eq!(part.parent_path.as_deref(), Some("/dev/nvme0n1"));
    }
}
