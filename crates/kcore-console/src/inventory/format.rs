//! Human-readable size formatting (binary IEC units).

const KIB: u128 = 1024;
const MIB: u128 = 1024 * KIB;
const GIB: u128 = 1024 * MIB;
const TIB: u128 = 1024 * GIB;
const PIB: u128 = 1024 * TIB;

/// Formats a byte count like `931.5 GiB` or `1.80 TiB` (1 decimal, adaptive unit).
pub fn format_bytes(n: u64) -> String {
    format_u128(n as u128)
}

fn format_u128(n: u128) -> String {
    if n < KIB {
        return format!("{n} B");
    }
    let f = n as f64;
    if n < MIB {
        return format!("{:.1} KiB", f / KIB as f64);
    }
    if n < GIB {
        return format!("{:.1} MiB", f / MIB as f64);
    }
    if n < TIB {
        return format!("{:.1} GiB", f / GIB as f64);
    }
    if n < PIB {
        return format!("{:.2} TiB", f / TIB as f64);
    }
    format!("{:.2} PiB", f / PIB as f64)
}

#[cfg(test)]
mod tests {
    use super::format_bytes;

    #[test]
    fn small_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn kib() {
        assert_eq!(format_bytes(1024), "1.0 KiB");
        let u = 1536;
        assert_eq!(format_bytes(u), "1.5 KiB");
    }

    #[test]
    fn gib() {
        // ~931.5 GiB for a 1 TB disk
        let n = 1000_u64 * 1000 * 1000 * 1000;
        let s = format_bytes(n);
        assert!(s.contains("GiB"), "{s}");
    }
}
