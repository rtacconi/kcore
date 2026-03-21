use serde::Deserialize;
use std::process::Command;

use crate::proto::NetworkInterfaceInfo;

#[derive(Deserialize)]
struct IpInterface {
    ifname: String,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    operstate: Option<String>,
    #[serde(default)]
    mtu: Option<i32>,
    #[serde(default)]
    addr_info: Vec<AddrInfo>,
}

#[derive(Deserialize)]
struct AddrInfo {
    #[serde(default)]
    local: Option<String>,
    #[serde(default)]
    prefixlen: Option<u32>,
}

pub fn list_network_interfaces() -> Result<Vec<NetworkInterfaceInfo>, String> {
    let output = Command::new("ip")
        .args(["-j", "address", "show"])
        .output()
        .map_err(|e| format!("running ip: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "ip failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let parsed: Vec<IpInterface> =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("parsing ip JSON: {e}"))?;

    Ok(parsed
        .into_iter()
        .map(|iface| {
            let addresses = iface
                .addr_info
                .iter()
                .filter_map(|a| {
                    let local = a.local.as_deref()?;
                    let prefix = a.prefixlen.unwrap_or(0);
                    Some(format!("{local}/{prefix}"))
                })
                .collect();

            NetworkInterfaceInfo {
                name: iface.ifname,
                mac_address: iface.address.unwrap_or_default(),
                state: iface.operstate.unwrap_or_default(),
                mtu: iface.mtu.unwrap_or(0),
                addresses,
            }
        })
        .collect())
}
