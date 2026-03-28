use crate::controller_proto;
use crate::node_proto;

pub fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn short_vm_id_seed() -> String {
    let raw = uuid_v4();
    let start = raw.len().saturating_sub(8);
    raw[start..].to_string()
}

pub fn controller_state_from_node_state(state: i32) -> i32 {
    match node_proto::VmState::try_from(state).unwrap_or(node_proto::VmState::Unknown) {
        node_proto::VmState::Unknown => controller_proto::VmState::Unknown as i32,
        node_proto::VmState::Stopped => controller_proto::VmState::Stopped as i32,
        node_proto::VmState::Running => controller_proto::VmState::Running as i32,
        node_proto::VmState::Paused => controller_proto::VmState::Paused as i32,
        node_proto::VmState::Error => controller_proto::VmState::Error as i32,
    }
}

pub fn state_fallback_without_runtime(auto_start: bool) -> i32 {
    if auto_start {
        controller_proto::VmState::Unknown as i32
    } else {
        controller_proto::VmState::Stopped as i32
    }
}

pub fn parse_datetime_to_timestamp(dt: &str) -> Option<prost_types::Timestamp> {
    let parts: Vec<&str> = dt.split(['-', ' ', ':']).collect();
    if parts.len() < 6 {
        return None;
    }
    let year: i32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;
    let hour: u32 = parts[3].parse().ok()?;
    let min: u32 = parts[4].parse().ok()?;
    let sec: u32 = parts[5].parse().ok()?;

    let days = days_from_civil(year, month, day);
    let secs = days * 86400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64;
    Some(prost_types::Timestamp {
        seconds: secs,
        nanos: 0,
    })
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = m as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

pub fn parse_port_list(s: &str) -> Vec<i32> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(',').filter_map(|p| p.trim().parse().ok()).collect()
}

/// Deterministic VNI from network name. Range 10000–15999.
pub fn compute_vni(name: &str) -> i32 {
    let mut hash: u32 = 5381;
    for b in name.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u32);
    }
    10000 + (hash % 6000) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_vni_stays_in_range() {
        for name in ["overlay", "prod", "test-vxlan", "a", "zzzzz"] {
            let vni = compute_vni(name);
            assert!(vni >= 10000, "vni {vni} below 10000 for '{name}'");
            assert!(vni < 16000, "vni {vni} above 15999 for '{name}'");
        }
    }

    #[test]
    fn compute_vni_is_deterministic() {
        assert_eq!(compute_vni("overlay"), compute_vni("overlay"));
    }
}
