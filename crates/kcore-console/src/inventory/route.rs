//! Default-route interface from `/proc/net/route` (IPv4).

/// Returns the interface name that carries the lowest-metric IPv4 default route, if any.
pub fn default_route_ifname(route_table: &str) -> Option<String> {
    let mut best_metric: u32 = u32::MAX;
    let mut best: Option<String> = None;
    for line in route_table.lines().skip(1) {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.len() < 8 {
            continue;
        }
        if p[1] != "00000000" {
            continue;
        }
        let ifname = p[0].to_string();
        let metric = p[6].parse::<u32>().unwrap_or(0);
        if metric < best_metric {
            best_metric = metric;
            best = Some(ifname);
        } else if metric == best_metric && best.is_none() {
            best = Some(ifname);
        }
    }
    best
}

/// Reads `/proc/net/route` and returns the default interface name.
pub fn read_default_ifname() -> Option<String> {
    std::fs::read_to_string("/proc/net/route")
        .ok()
        .and_then(|s| default_route_ifname(&s))
}

#[cfg(test)]
mod tests {
    use super::default_route_ifname;

    const FIXTURE: &str = "Iface	Destination	Gateway 	Flags	RefCnt	Use	Metric	Mask		MTU	Window	IRTT
eth0	00000000	0101A8C0	0003	0	0	0	00000000	0	0	0
eth1	00000000	00000000	0001	0	0	5	00000000	0	0	0
";

    #[test]
    fn picks_lowest_metric_default() {
        let ifn = default_route_ifname(FIXTURE);
        assert_eq!(ifn.as_deref(), Some("eth0"));
    }
}
