//! Hostname, uptime, version, management URL, and optional kcore cluster metadata.

use chrono::Local;
use std::time::Duration;

use super::api::ApiStatus;

/// Build-time optional git short hash.
pub const GIT_SHORT: &str = match option_env!("KCORE_GIT_REV") {
    Some(s) => s,
    None => "",
};

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct Meta {
    pub product: String,
    pub version: String,
    pub build_id: String,
    pub hostname: String,
    pub uptime: Duration,
    pub uptime_str: String,
    pub local_time: String,
    pub management_url: String,
    pub api_endpoint: String,
    pub api_status: ApiStatus,
    pub cluster_name: String,
    pub node_role: String,
    pub health: Health,
    pub local_login: &'static str,
    pub remote_hint: String,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            product: "kcore hypervisor".to_string(),
            version: PKG_VERSION.to_string(),
            build_id: if GIT_SHORT.is_empty() {
                "—".to_string()
            } else {
                GIT_SHORT.to_string()
            },
            hostname: "loading".to_string(),
            uptime: Duration::from_secs(0),
            uptime_str: "—".to_string(),
            local_time: "—".to_string(),
            management_url: "—".to_string(),
            api_endpoint: "127.0.0.1:9091".to_string(),
            api_status: ApiStatus::Unavailable,
            cluster_name: "—".to_string(),
            node_role: "—".to_string(),
            health: Health::Unknown,
            local_login: "disabled",
            remote_hint: "kcorectl login https://<node-ip>:8443".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ok,
    Degraded,
    Unknown,
    Critical,
}

impl std::fmt::Display for Health {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Health::Ok => "OK",
            Health::Degraded => "Degraded",
            Health::Unknown => "Unknown",
            Health::Critical => "Critical",
        };
        write!(f, "{s}")
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

fn file_first_line(p: &str) -> Option<String> {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|s| !s.is_empty())
}

fn run_ip_json() -> Option<serde_json::Value> {
    let out = std::process::Command::new("ip")
        .args(["-j", "address", "show"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

fn parse_primary_ipv4(v: &serde_json::Value, default_if: &str) -> Option<String> {
    let arr = v.as_array()?;
    for iface in arr {
        if iface.get("ifname")?.as_str()? != default_if {
            continue;
        }
        for a in iface.get("addr_info")?.as_array()? {
            if a.get("family")?.as_str()? != "inet" {
                continue;
            }
            if a.get("scope")?.as_str()? == "global" {
                return a.get("local")?.as_str().map(String::from);
            }
        }
    }
    None
}

/// Primary IPv4 for management URL, best-effort.
fn primary_ipv4_for_mgmt() -> Option<String> {
    if let Some(ip) = env("KCORE_MANAGEMENT_IP") {
        return Some(ip);
    }
    let ifname = super::route::read_default_ifname()?;
    let j = run_ip_json()?;
    parse_primary_ipv4(&j, &ifname)
}

fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    let days = s / 86400;
    let h = (s % 86400) / 3600;
    let m = (s % 3600) / 60;
    if days > 0 {
        format!("{days}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m {sec}s", sec = s % 60)
    }
}

/// Snapshot metadata. Tolerates missing /proc, tools, and files.
pub fn load_meta(api: &ApiStatus) -> Meta {
    let hostname = std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".into());

    let mut uptime = Duration::from_secs(0);
    if let Ok(c) = std::fs::read_to_string("/proc/uptime") {
        if let Some(f) = c.split_whitespace().next() {
            if let Ok(s) = f.parse::<f64>() {
                uptime = Duration::from_secs_f64(s);
            }
        }
    }
    let uptime_str = format_duration(uptime);

    let kcore_version = std::fs::read_to_string("/run/kcore/version")
        .or_else(|_| std::fs::read_to_string("/etc/kcore/version"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let version = if !kcore_version.is_empty() {
        kcore_version
    } else {
        PKG_VERSION.to_string()
    };

    let build_id = if !GIT_SHORT.is_empty() {
        GIT_SHORT.to_string()
    } else {
        "—".to_string()
    };

    let cluster_name = env("KCORE_CLUSTER_NAME")
        .or_else(|| file_first_line("/etc/kcore/cluster_name"))
        .unwrap_or_else(|| "—".to_string());

    let node_role = env("KCORE_NODE_ROLE")
        .or_else(|| file_first_line("/etc/kcore/node_role"))
        .unwrap_or_else(|| "—".to_string());

    let management_url = if let Some(u) = env("KCORE_MANAGEMENT_URL") {
        u
    } else if let Some(ip) = primary_ipv4_for_mgmt() {
        let port = env("KCORE_MANAGEMENT_PORT").unwrap_or_else(|| "8443".into());
        format!("https://{ip}:{port}")
    } else {
        "—".to_string()
    };

    let health = match api {
        ApiStatus::Reachable { healthy } if *healthy => Health::Ok,
        ApiStatus::Reachable { .. } => Health::Degraded,
        _ => Health::Unknown,
    };
    let api_endpoint = format!(
        "127.0.0.1:{}",
        env("KCORE_API_PORT").unwrap_or_else(|| super::api::KCORE_API_PORT.to_string())
    );

    let local_time = Local::now().format("%Y-%m-%d %H:%M:%S (local)").to_string();

    let product = "kcore hypervisor".to_string();
    let remote = if management_url == "—" {
        "kcorectl login https://<node-ip>:8443".to_string()
    } else {
        let with_scheme =
            if management_url.starts_with("https://") || management_url.starts_with("http://") {
                management_url.clone()
            } else {
                format!("https://{}", management_url.trim_start_matches('/'))
            };
        format!("kcorectl login {with_scheme}")
    };

    Meta {
        product,
        version,
        build_id,
        hostname,
        uptime,
        uptime_str,
        local_time,
        management_url,
        api_endpoint,
        api_status: api.clone(),
        cluster_name,
        node_role,
        health,
        local_login: "disabled",
        remote_hint: remote,
    }
}
