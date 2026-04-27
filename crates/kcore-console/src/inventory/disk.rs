//! Block device inventory (Linux, `lsblk` JSON) with heuristic usage role.

use serde_json::Value;

use super::format::format_bytes;

#[derive(Debug, Clone, Default)]
pub struct Disk {
    pub name: String,
    pub path: String,
    pub model: String,
    pub serial: String,
    pub size: u64,
    pub size_text: String,
    pub kind: String,
    pub ro: String,
    pub mountpoints: String,
    pub health: String,
    pub usage_role: String,
}

fn kind_from(rot: Option<bool>, model: &str) -> String {
    let t = model.to_lowercase();
    if t.contains("nvme") {
        return "NVMe".to_string();
    }
    if rot == Some(true) {
        "HDD".to_string()
    } else {
        "SSD".to_string()
    }
}

fn walk_mounts(v: &Value, acc: &mut Vec<String>) {
    if let Some(pts) = v.get("mountpoints").and_then(|x| x.as_array()) {
        for p in pts {
            if let Some(s) = p.as_str() {
                if !s.is_empty() {
                    acc.push(s.to_string());
                }
            }
        }
    }
    if let Some(s) = v.get("mountpoint").and_then(|x| x.as_str()) {
        if !s.is_empty() {
            acc.push(s.to_string());
        }
    }
    if let Some(ch) = v.get("children").and_then(|x| x.as_array()) {
        for c in ch {
            walk_mounts(c, acc);
        }
    }
}

fn usage_role(mounts: &str) -> String {
    if mounts == "—" || mounts.is_empty() {
        return "unknown".to_string();
    }
    if mounts == "/" || mounts.split(',').any(|m| m.trim() == "/") {
        return "system".to_string();
    }
    if mounts.contains("/var/lib") || mounts.to_lowercase().contains("kcore") {
        return "VM storage (hint)".to_string();
    }
    "unknown".to_string()
}

/// Parse `lsblk -J -b` and list top-level `disk` devices.
pub fn list_disks() -> Vec<Disk> {
    let out = match std::process::Command::new("lsblk")
        .args([
            "-J",
            "-b",
            "-o",
            "NAME,PATH,TYPE,SIZE,ROTA,RO,MODEL,SERIAL,WWN,MOUNTPOINTS,MOUNTPOINT,FSTYPE,STATE",
        ])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    parse_disks_from_lsblk_json(&out.stdout)
}

pub(crate) fn parse_disks_from_lsblk_json(json: &[u8]) -> Vec<Disk> {
    let v: Value = match serde_json::from_slice(json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(top) = v.get("blockdevices").and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for d in top {
        if d.get("type").and_then(|t| t.as_str()) != Some("disk") {
            continue;
        }
        let name = d
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown")
            .to_string();
        let path: String = d
            .get("path")
            .and_then(|x| x.as_str())
            .map(String::from)
            .unwrap_or_else(|| format!("/dev/{name}"));
        let size = d.get("size").and_then(|x| x.as_u64()).unwrap_or(0);
        let ro: String = d
            .get("ro")
            .and_then(|x| x.as_bool())
            .map(|b| (if b { "ro" } else { "rw" }).to_string())
            .unwrap_or_else(|| "—".to_string());
        let rota = d.get("rota").and_then(|x| x.as_bool());
        let model = d
            .get("model")
            .and_then(|x| x.as_str())
            .unwrap_or("—")
            .trim()
            .to_string();
        let serial = d
            .get("serial")
            .or_else(|| d.get("wwn"))
            .and_then(|x| x.as_str())
            .unwrap_or("—")
            .trim()
            .to_string();
        let mut mps: Vec<String> = Vec::new();
        walk_mounts(d, &mut mps);
        mps.sort();
        mps.dedup();
        let mountstr = if mps.is_empty() {
            "—".to_string()
        } else {
            mps.join(", ")
        };
        let health = d
            .get("health")
            .or_else(|| d.get("state"))
            .and_then(|x| x.as_str())
            .unwrap_or("—")
            .to_string();
        let size_text = if size == 0 {
            "—".to_string()
        } else {
            format_bytes(size)
        };
        let model_s = if model.is_empty() {
            "—".to_string()
        } else {
            model
        };
        let serial_s = if serial.is_empty() {
            "—".to_string()
        } else {
            serial
        };
        rows.push(Disk {
            name: name.clone(),
            path,
            model: model_s,
            serial: serial_s,
            size,
            size_text,
            kind: kind_from(rota, d.get("model").and_then(|m| m.as_str()).unwrap_or("")),
            ro,
            mountpoints: mountstr.clone(),
            health,
            usage_role: usage_role(&mountstr),
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::parse_disks_from_lsblk_json;

    #[test]
    fn parses_top_level_disks_and_mount_roles() {
        let fixture = br#"{
          "blockdevices": [
            {
              "name": "nvme0n1", "path": "/dev/nvme0n1", "type": "disk",
              "size": 2000398934016, "rota": false, "ro": false,
              "model": "Fast NVMe", "serial": "ABC123",
              "mountpoints": [null],
              "children": [
                {"name": "nvme0n1p1", "type": "part", "mountpoints": ["/"]}
              ]
            },
            {
              "name": "sda", "path": "/dev/sda", "type": "disk",
              "size": 1000000000000, "rota": true, "ro": false,
              "model": "Archive HDD", "serial": null,
              "mountpoints": [null]
            }
          ]
        }"#;

        let disks = parse_disks_from_lsblk_json(fixture);
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].kind, "NVMe");
        assert_eq!(disks[0].usage_role, "system");
        assert_eq!(disks[1].kind, "HDD");
        assert_eq!(disks[1].serial, "—");
    }
}
