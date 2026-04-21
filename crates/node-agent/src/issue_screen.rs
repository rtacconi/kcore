use crate::discovery::{list_disks, list_network_interfaces};
use crate::proto::{DiskInfo, NetworkInterfaceInfo};

const MAX_NICS: usize = 8;
const MAX_DISKS: usize = 8;

const RESET: &str = "\x1b[0m";
const BG_DARK: &str = "\x1b[48;5;17m";
const FG_TITLE: &str = "\x1b[38;5;117m";
const FG_ACCENT: &str = "\x1b[38;5;45m";
const FG_TEXT: &str = "\x1b[38;5;252m";
const FG_MUTED: &str = "\x1b[38;5;110m";
const FG_WARN: &str = "\x1b[38;5;215m";

pub fn should_use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM")
            .map(|term| term != "dumb")
            .unwrap_or(true)
}

pub fn render_issue(use_color: bool) -> String {
    let disks = list_disks();
    let nics = list_network_interfaces();

    let mut lines = vec![];
    lines.push(style(
        use_color,
        format!("{BG_DARK}{FG_TITLE} kcoreOS Node Console {RESET}"),
    ));
    lines.push(style(
        use_color,
        format!("{FG_ACCENT} Declarative Virtualization Hypervisor {RESET}"),
    ));
    lines.push(String::new());
    lines.push(style(
        use_color,
        format!("{FG_TEXT} Username: root  Password: kcore {RESET}"),
    ));
    lines.push(style(
        use_color,
        format!("{FG_MUTED} Kernel \\r on an \\m (\\l) {RESET}"),
    ));
    lines.push(String::new());

    lines.push(style(
        use_color,
        format!("{FG_ACCENT} Network Interfaces {RESET}"),
    ));
    lines.push(style(
        use_color,
        format!(
            "{FG_MUTED} {name:<12} {state:<8} {mac:<18} {addr}{RESET}",
            name = "NAME",
            state = "STATE",
            mac = "MAC",
            addr = "PRIMARY ADDRESS"
        ),
    ));
    match nics {
        Ok(interfaces) => {
            let visible = interfaces
                .into_iter()
                .filter(|iface| iface.name != "lo")
                .collect::<Vec<_>>();
            render_nics(&mut lines, visible, use_color);
        }
        Err(err) => lines.push(style(
            use_color,
            format!("{FG_WARN} NIC discovery unavailable: {err}{RESET}"),
        )),
    }
    lines.push(String::new());

    lines.push(style(use_color, format!("{FG_ACCENT} Disks {RESET}")));
    lines.push(style(
        use_color,
        format!(
            "{FG_MUTED} {path:<18} {size:<8} {model}{RESET}",
            path = "PATH",
            size = "SIZE",
            model = "MODEL"
        ),
    ));
    match disks {
        Ok(devices) => render_disks(&mut lines, devices, use_color),
        Err(err) => lines.push(style(
            use_color,
            format!("{FG_WARN} Disk discovery unavailable: {err}{RESET}"),
        )),
    }

    lines.push(String::new());
    lines.push(style(
        use_color,
        format!("{FG_MUTED} Log in to continue configuration and cluster join. {RESET}"),
    ));
    lines.join("\n") + "\n"
}

fn render_nics(
    lines: &mut Vec<String>,
    mut interfaces: Vec<NetworkInterfaceInfo>,
    use_color: bool,
) {
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));
    let count = interfaces.len();
    for iface in interfaces.into_iter().take(MAX_NICS) {
        let primary = iface
            .addresses
            .iter()
            .find(|addr| !addr.starts_with("fe80"))
            .or_else(|| iface.addresses.first())
            .cloned()
            .unwrap_or_else(|| "-".to_string());
        lines.push(style(
            use_color,
            format!(
                "{FG_TEXT} {name:<12} {state:<8} {mac:<18} {addr}{RESET}",
                name = truncate(&iface.name, 12),
                state = truncate(&iface.state, 8),
                mac = truncate(&iface.mac_address, 18),
                addr = primary
            ),
        ));
    }
    if count > MAX_NICS {
        lines.push(style(
            use_color,
            format!(
                "{FG_MUTED} ... plus {} additional interface(s){RESET}",
                count - MAX_NICS
            ),
        ));
    }
    if count == 0 {
        lines.push(style(
            use_color,
            format!("{FG_WARN} No interfaces detected.{RESET}"),
        ));
    }
}

fn render_disks(lines: &mut Vec<String>, mut devices: Vec<DiskInfo>, use_color: bool) {
    devices.sort_by(|a, b| a.path.cmp(&b.path));
    let count = devices.len();
    for disk in devices.into_iter().take(MAX_DISKS) {
        let model = if disk.model.is_empty() {
            "-".to_string()
        } else {
            disk.model
        };
        lines.push(style(
            use_color,
            format!(
                "{FG_TEXT} {path:<18} {size:<8} {model}{RESET}",
                path = truncate(&disk.path, 18),
                size = truncate(&disk.size, 8),
                model = truncate(&model, 42)
            ),
        ));
    }
    if count > MAX_DISKS {
        lines.push(style(
            use_color,
            format!(
                "{FG_MUTED} ... plus {} additional disk(s){RESET}",
                count - MAX_DISKS
            ),
        ));
    }
    if count == 0 {
        lines.push(style(
            use_color,
            format!("{FG_WARN} No disks detected.{RESET}"),
        ));
    }
}

fn truncate(input: &str, width: usize) -> String {
    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() <= width {
        return input.to_string();
    }
    if width <= 3 {
        return ".".to_string();
    }
    chars[..width - 3].iter().collect::<String>() + "..."
}

fn style(use_color: bool, value: String) -> String {
    if use_color {
        value
    } else {
        value
            .replace(BG_DARK, "")
            .replace(FG_TITLE, "")
            .replace(FG_ACCENT, "")
            .replace(FG_TEXT, "")
            .replace(FG_MUTED, "")
            .replace(FG_WARN, "")
            .replace(RESET, "")
    }
}
