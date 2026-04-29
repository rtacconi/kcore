//! systemd / service stub status for the Diagnostics page.

use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct ServiceLine {
    pub name: String,
    pub status: String,
}

/// Best-effort `systemctl is-active` for kcore units.
pub fn kcore_diagnostics() -> Vec<ServiceLine> {
    const UNITS: &[(&str, &str)] = &[
        ("kcore-node-agent", "kcore-node-agent.service"),
        ("kcore-controller", "kcore-controller.service"),
        ("kcore-dashboard", "kcore-dashboard.service"),
    ];
    let mut v = Vec::new();
    let has_systemctl = which_systemctl();
    for (label, unit) in UNITS {
        let status = if has_systemctl {
            if let Some(out) = run_active(unit) {
                if out == "active" {
                    "healthy".to_string()
                } else {
                    format!("{out} (not installed or inactive)")
                }
            } else {
                "—".to_string()
            }
        } else {
            "— (no systemctl)".to_string()
        };
        v.push(ServiceLine {
            name: (*label).to_string(),
            status,
        });
    }
    v
}

fn which_systemctl() -> bool {
    Command::new("systemctl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_active(unit: &str) -> Option<String> {
    let o = Command::new("systemctl")
        .args(["is-active", unit])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
