//! Network interface inventory (Linux, `ip -j` with sysfs fallbacks).

use serde::Deserialize;

use super::route::read_default_ifname;

fn dash(s: &str) -> String {
    if s.is_empty() {
        "—".to_string()
    } else {
        s.to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Nic {
    pub name: String,
    pub mac: String,
    pub oper_state: String,
    pub ipv4: String,
    pub ipv6: String,
    pub mtu: String,
    pub speed: String,
    pub driver: String,
    pub default_route: bool,
    pub management: bool,
}

#[derive(Debug, Deserialize)]
struct IpEntry {
    ifname: String,
    addr_info: Option<Vec<AddrInfo>>,
}

#[derive(Debug, Deserialize)]
struct AddrInfo {
    family: String,
    local: String,
    scope: Option<String>,
}

pub fn list_nics() -> Vec<Nic> {
    let out = std::process::Command::new("ip")
        .args(["-j", "link", "show"])
        .output();
    let links: Vec<serde_json::Value> = out
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or_default();

    let out_a = std::process::Command::new("ip")
        .args(["-j", "address", "show"])
        .output();
    let addr_json: Option<Vec<IpEntry>> = out_a
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice(&o.stdout).ok());

    let default_if = read_default_ifname();
    let mgmt_if = default_if.clone().or_else(|| {
        std::env::var("KCORE_MGMT_IFACE")
            .ok()
            .or_else(|| std::env::var("KCORE_MANAGEMENT_IF").ok())
    });

    let mut rows = build_nics_from_links(
        &links,
        addr_json.as_deref(),
        default_if.as_deref(),
        mgmt_if.as_deref(),
    );
    if rows.is_empty() {
        rows.extend(sysfs_enum_fallback());
    }
    rows
}

fn build_nics_from_links(
    links: &[serde_json::Value],
    addr_json: Option<&[IpEntry]>,
    default_if: Option<&str>,
    mgmt_if: Option<&str>,
) -> Vec<Nic> {
    let mut rows = Vec::new();
    for link in links {
        let ifname = link
            .get("ifname")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if ifname.is_empty() || ifname == "lo" {
            continue;
        }
        let mtu = link
            .get("mtu")
            .and_then(|v| v.as_u64())
            .map(|m| m.to_string())
            .unwrap_or_else(|| "—".to_string());
        let state = link
            .get("operstate")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let mac: String = link
            .get("address")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| read_sysfs_mac(&ifname))
            .unwrap_or_else(|| "—".to_string());
        let (v4, v6) = extract_addrs(&ifname, addr_json);
        let speed = read_link_speed(&ifname);
        let driver = read_driver(&ifname);
        let default_route = default_if.map(|d| d == ifname).unwrap_or(false);
        let management = mgmt_if
            .map(|m| m == ifname)
            .unwrap_or_else(|| default_route);

        rows.push(Nic {
            name: ifname,
            mac: if mac.is_empty() {
                "—".to_string()
            } else {
                mac
            },
            oper_state: dash(state),
            ipv4: v4,
            ipv6: v6,
            mtu: dash(&mtu),
            speed: dash(&speed),
            driver: dash(&driver),
            default_route,
            management,
        });
    }
    rows
}

fn extract_addrs(ifname: &str, entries: Option<&[IpEntry]>) -> (String, String) {
    let mut v4 = "—".to_string();
    let mut v6 = "—".to_string();
    let Some(entries) = entries else {
        return (v4, v6);
    };
    for e in entries {
        if e.ifname != ifname {
            continue;
        }
        for a in e.addr_info.as_deref().unwrap_or(&[]) {
            if a.family == "inet" {
                if a.scope.as_deref() == Some("global") || v4 == "—" {
                    v4 = a.local.clone();
                }
            } else if a.family == "inet6" {
                if a.local == "::1" {
                    continue;
                }
                if a.scope.as_deref() == Some("global") || v6 == "—" {
                    v6 = a.local.clone();
                }
            }
        }
    }
    (v4, v6)
}

fn read_link_speed(ifname: &str) -> String {
    let p = format!("/sys/class/net/{ifname}/speed");
    match std::fs::read_to_string(&p) {
        Ok(s) => {
            let t = s.trim();
            if t == "-1" {
                "—".to_string()
            } else {
                format!("{t} Mbps")
            }
        }
        Err(_) => "—".to_string(),
    }
}

fn read_driver(ifname: &str) -> String {
    // /sys/class/net/DEVICE/uevent has DRIVER= or we follow device/ driver link
    let p = format!("/sys/class/net/{ifname}/device/uevent");
    if let Ok(s) = std::fs::read_to_string(p) {
        for l in s.lines() {
            if let Some(d) = l.strip_prefix("DRIVER=") {
                return d.to_string();
            }
        }
    }
    "—".to_string()
}

fn read_sysfs_mac(ifname: &str) -> Option<String> {
    std::fs::read_to_string(format!("/sys/class/net/{ifname}/address"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Very small fallback: enumerate /sys/class/net
fn sysfs_enum_fallback() -> Vec<Nic> {
    let mut n = Vec::new();
    let Ok(dir) = std::fs::read_dir("/sys/class/net") else {
        return n;
    };
    for e in dir.flatten() {
        let ifname = e.file_name().to_string_lossy().to_string();
        if ifname == "lo" {
            continue;
        }
        n.push(Nic {
            name: ifname,
            ..Default::default()
        });
    }
    n
}

#[cfg(test)]
mod tests {
    use super::{build_nics_from_links, IpEntry};

    #[test]
    fn parses_link_and_address_json_with_default_and_management_markers() {
        let links: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
              {"ifname":"lo","mtu":65536,"operstate":"UNKNOWN","address":"00:00:00:00:00:00"},
              {"ifname":"eth0","mtu":1500,"operstate":"UP","address":"aa:bb:cc:dd:ee:ff"},
              {"ifname":"eth1","mtu":9000,"operstate":"DOWN","address":"11:22:33:44:55:66"}
            ]"#,
        )
        .unwrap();
        let addrs: Vec<IpEntry> = serde_json::from_str(
            r#"[
              {"ifname":"eth0","addr_info":[
                {"family":"inet","local":"10.190.15.20","scope":"global"},
                {"family":"inet6","local":"fe80::1","scope":"link"},
                {"family":"inet6","local":"fd00::20","scope":"global"}
              ]},
              {"ifname":"eth1","addr_info":[]}
            ]"#,
        )
        .unwrap();

        let nics = build_nics_from_links(&links, Some(&addrs), Some("eth0"), Some("eth0"));
        assert_eq!(nics.len(), 2);
        assert_eq!(nics[0].name, "eth0");
        assert_eq!(nics[0].ipv4, "10.190.15.20");
        assert_eq!(nics[0].ipv6, "fd00::20");
        assert!(nics[0].default_route);
        assert!(nics[0].management);
        assert_eq!(nics[1].ipv4, "—");
    }
}
