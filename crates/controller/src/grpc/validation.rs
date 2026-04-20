use crate::controller_proto;
use std::net::Ipv4Addr;
use tonic::Status;

pub fn validate_image_sha256(sha: &str) -> Result<String, Status> {
    let normalized = sha.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Status::invalid_argument(
            "image_sha256 must be exactly 64 hexadecimal characters",
        ));
    }
    Ok(normalized)
}

pub fn validate_image_url(url: &str) -> Result<String, Status> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument("image_url is required"));
    }
    if !trimmed.starts_with("https://") {
        return Err(Status::invalid_argument(
            "image_url must use https:// scheme",
        ));
    }
    Ok(trimmed.to_string())
}

fn sanitize_image_file_name(url: &str) -> String {
    let raw_name = url.rsplit('/').next().unwrap_or("image.raw");
    let cleaned: String = raw_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "image.raw".to_string()
    } else {
        cleaned
    }
}

pub fn derive_local_image_path(image_url: &str, image_sha256: &str) -> String {
    let file_name = sanitize_image_file_name(image_url);
    format!(
        "/var/lib/kcore/images/{}-{}",
        &image_sha256[..12],
        file_name
    )
}

pub fn derive_image_format(image_url: &str) -> String {
    let lower = image_url.to_ascii_lowercase();
    if lower.ends_with(".qcow2") || lower.ends_with(".qcow") {
        "qcow2".to_string()
    } else {
        "raw".to_string()
    }
}

pub fn validate_image_path(path: &str) -> Result<String, Status> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument("image_path is required"));
    }
    if !trimmed.starts_with('/') {
        return Err(Status::invalid_argument("image_path must be absolute"));
    }
    if trimmed.contains("..") {
        return Err(Status::invalid_argument(
            "image_path must not contain path traversal",
        ));
    }
    Ok(trimmed.to_string())
}

pub fn normalize_image_format(format: &str) -> Result<String, Status> {
    let normalized = format.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "raw" | "qcow2" => Ok(normalized),
        _ => Err(Status::invalid_argument(
            "image_format must be 'raw' or 'qcow2'",
        )),
    }
}

pub fn derive_image_format_from_path(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".qcow2") || lower.ends_with(".qcow") {
        "qcow2".to_string()
    } else {
        "raw".to_string()
    }
}

pub fn validate_network_type(network_type: &str) -> Result<String, Status> {
    let trimmed = network_type.trim();
    if trimmed.is_empty() {
        return Ok("nat".to_string());
    }
    match trimmed {
        "nat" | "bridge" | "vxlan" => Ok(trimmed.to_string()),
        _ => Err(Status::invalid_argument(
            "network_type must be 'nat', 'bridge', or 'vxlan'",
        )),
    }
}

pub fn validate_network_name(name: &str) -> Result<String, Status> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument("network name is required"));
    }
    if trimmed == "default" {
        return Err(Status::invalid_argument(
            "network name 'default' is reserved; configure it via controller defaultNetwork",
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Status::invalid_argument(
            "network name must contain only letters, digits, '-' or '_'",
        ));
    }
    Ok(trimmed.to_string())
}

pub fn validate_ipv4(value: &str, field: &str) -> Result<String, Status> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    trimmed
        .parse::<Ipv4Addr>()
        .map_err(|_| Status::invalid_argument(format!("{field} must be a valid IPv4 address")))?;
    Ok(trimmed.to_string())
}

pub fn validate_netmask(value: &str) -> Result<String, Status> {
    let parsed = validate_ipv4(value, "internal_netmask")?;
    let bits =
        u32::from(parsed.parse::<Ipv4Addr>().map_err(|_| {
            Status::invalid_argument("internal_netmask must be a valid IPv4 address")
        })?);
    let mut seen_zero = false;
    for i in 0..32 {
        let bit = (bits >> (31 - i)) & 1;
        if bit == 0 {
            seen_zero = true;
        } else if seen_zero {
            return Err(Status::invalid_argument(
                "internal_netmask must be contiguous (for example 255.255.255.0)",
            ));
        }
    }
    Ok(parsed)
}

pub fn normalize_storage_backend(backend: i32, required: bool) -> Result<String, Status> {
    let parsed = controller_proto::StorageBackendType::try_from(backend)
        .unwrap_or(controller_proto::StorageBackendType::Unspecified);
    match parsed {
        controller_proto::StorageBackendType::Filesystem => Ok("filesystem".to_string()),
        controller_proto::StorageBackendType::Lvm => Ok("lvm".to_string()),
        controller_proto::StorageBackendType::Zfs => Ok("zfs".to_string()),
        controller_proto::StorageBackendType::Unspecified if required => Err(
            Status::invalid_argument("storage_backend is required (filesystem|lvm|zfs)"),
        ),
        controller_proto::StorageBackendType::Unspecified => Ok("filesystem".to_string()),
    }
}

pub fn storage_backend_to_proto(backend: &str) -> i32 {
    match backend.trim().to_ascii_lowercase().as_str() {
        "filesystem" => controller_proto::StorageBackendType::Filesystem as i32,
        "lvm" => controller_proto::StorageBackendType::Lvm as i32,
        "zfs" => controller_proto::StorageBackendType::Zfs as i32,
        _ => controller_proto::StorageBackendType::Unspecified as i32,
    }
}

pub fn validate_storage_size_bytes(size_bytes: i64) -> Result<i64, Status> {
    if size_bytes <= 0 {
        return Err(Status::invalid_argument("storage_size_bytes must be > 0"));
    }
    Ok(size_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_path_must_be_absolute_without_traversal() {
        let ok = validate_image_path("/var/lib/kcore/images/base.raw").expect("valid path");
        assert_eq!(ok, "/var/lib/kcore/images/base.raw");

        let not_absolute = validate_image_path("var/lib/kcore/images/base.raw");
        assert!(not_absolute.is_err());

        let traversal = validate_image_path("/var/lib/kcore/images/../evil.raw");
        assert!(traversal.is_err());
    }

    #[test]
    fn normalize_image_format_accepts_only_raw_or_qcow2() {
        assert_eq!(
            normalize_image_format("RAW").expect("raw should normalize"),
            "raw"
        );
        assert_eq!(
            normalize_image_format("qcow2").expect("qcow2 should normalize"),
            "qcow2"
        );
        assert!(normalize_image_format("iso").is_err());
    }

    #[test]
    fn normalize_storage_backend_enforces_required_flag() {
        assert_eq!(
            normalize_storage_backend(
                controller_proto::StorageBackendType::Filesystem as i32,
                true
            )
            .expect("filesystem"),
            "filesystem"
        );
        assert_eq!(
            normalize_storage_backend(controller_proto::StorageBackendType::Lvm as i32, true)
                .expect("lvm"),
            "lvm"
        );
        assert!(normalize_storage_backend(0, true).is_err());
        assert_eq!(
            normalize_storage_backend(0, false).expect("default"),
            "filesystem"
        );
    }

    #[test]
    fn validate_storage_size_bytes_requires_positive() {
        assert!(validate_storage_size_bytes(0).is_err());
        assert!(validate_storage_size_bytes(-1).is_err());
        assert_eq!(
            validate_storage_size_bytes(1024 * 1024).expect("positive"),
            1024 * 1024
        );
    }

    #[test]
    fn validate_network_name_rejects_default_keyword() {
        assert!(validate_network_name("default").is_err());
        assert!(validate_network_name(" default  ").is_err());
    }

    #[test]
    fn validate_network_type_accepts_valid_and_rejects_invalid() {
        assert_eq!(validate_network_type("").expect("empty -> nat"), "nat");
        assert_eq!(validate_network_type("nat").expect("nat"), "nat");
        assert_eq!(validate_network_type("bridge").expect("bridge"), "bridge");
        assert_eq!(validate_network_type("vxlan").expect("vxlan"), "vxlan");
        assert!(validate_network_type("invalid").is_err());
        assert!(validate_network_type("vlan").is_err());
    }
}

/// Property-based tests (Phase 2) — declarative input validation.
///
/// These validators sit on the perimeter of every controller RPC; the
/// strongest guarantees we want are: never panic on any input, never
/// accept syntactically-invalid input, and produce a normalized output
/// that is itself idempotent under re-validation.
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// Every validator must terminate without panicking on
        /// arbitrary 0..=128 byte input.
        #[test]
        fn validators_never_panic(s in ".{0,128}") {
            let _ = validate_image_sha256(&s);
            let _ = validate_image_url(&s);
            let _ = validate_image_path(&s);
            let _ = normalize_image_format(&s);
            let _ = validate_network_type(&s);
            let _ = validate_network_name(&s);
            let _ = validate_ipv4(&s, "field");
            let _ = validate_netmask(&s);
            let _ = storage_backend_to_proto(&s);
            let _ = derive_image_format(&s);
            let _ = derive_image_format_from_path(&s);
        }

        /// `validate_image_sha256` accepts a string iff its trimmed,
        /// lowercased form is exactly 64 ASCII hex chars. The accepted
        /// output is always already-normalized.
        #[test]
        fn validate_image_sha256_acceptance_matches_predicate(s in ".{0,80}") {
            let normalized = s.trim().to_ascii_lowercase();
            let predicate = normalized.len() == 64
                && normalized.chars().all(|c| c.is_ascii_hexdigit());
            let result = validate_image_sha256(&s);
            prop_assert_eq!(result.is_ok(), predicate);
            if let Ok(out) = result {
                prop_assert_eq!(out, normalized);
            }
        }

        /// Round-trip on canonical 64-hex inputs: regardless of case,
        /// any sequence of 64 hex digits parses and yields the
        /// lowercased version.
        #[test]
        fn validate_image_sha256_round_trip_on_random_hex(
            bytes in proptest::collection::vec(0u8..=15, 64),
            uppercase in any::<bool>(),
        ) {
            let mut s: String = bytes
                .iter()
                .map(|b| std::char::from_digit(*b as u32, 16).unwrap())
                .collect();
            if uppercase {
                s = s.to_ascii_uppercase();
            }
            let out = validate_image_sha256(&s).expect("hex must validate");
            prop_assert_eq!(out, s.to_ascii_lowercase());
        }

        /// `validate_image_url` accepts iff the trimmed input is
        /// non-empty and starts with `https://`. Anything else rejected.
        #[test]
        fn validate_image_url_requires_https(s in ".{0,64}") {
            let trimmed = s.trim();
            let predicate = !trimmed.is_empty() && trimmed.starts_with("https://");
            prop_assert_eq!(validate_image_url(&s).is_ok(), predicate);
        }

        /// `validate_image_path` rejects empty, non-absolute, and
        /// `..`-containing strings. Acceptance implies absolute path
        /// without traversal.
        #[test]
        fn validate_image_path_rejects_traversal(s in ".{0,64}") {
            let result = validate_image_path(&s);
            if let Ok(out) = result {
                prop_assert!(out.starts_with('/'));
                prop_assert!(!out.contains(".."));
                prop_assert!(!out.is_empty());
            }
        }

        /// `normalize_image_format` is case-insensitive and accepts
        /// **only** `raw` and `qcow2` (idempotent on output).
        #[test]
        fn normalize_image_format_accept_set(s in ".{0,16}") {
            let normalized = s.trim().to_ascii_lowercase();
            let predicate = normalized == "raw" || normalized == "qcow2";
            let result = normalize_image_format(&s);
            prop_assert_eq!(result.is_ok(), predicate);
            if let Ok(v) = result {
                prop_assert_eq!(normalize_image_format(&v).unwrap(), v);
            }
        }

        /// `validate_network_type` is case-sensitive (per implementation),
        /// accepts the empty string (defaulting to `nat`), and accepts
        /// exactly `nat` / `bridge` / `vxlan`. Output is idempotent.
        #[test]
        fn validate_network_type_accept_set(s in ".{0,16}") {
            let trimmed = s.trim();
            let predicate = trimmed.is_empty()
                || trimmed == "nat"
                || trimmed == "bridge"
                || trimmed == "vxlan";
            let result = validate_network_type(&s);
            prop_assert_eq!(result.is_ok(), predicate);
            if let Ok(v) = result {
                prop_assert!(v == "nat" || v == "bridge" || v == "vxlan");
                prop_assert_eq!(validate_network_type(&v).unwrap(), v);
            }
        }

        /// `validate_network_name` rejects empty, the reserved word
        /// `default`, and any name containing characters outside
        /// `[A-Za-z0-9_-]`.
        #[test]
        fn validate_network_name_rejects_invalid_chars(s in ".{0,32}") {
            let trimmed = s.trim();
            let chars_ok = !trimmed.is_empty()
                && trimmed
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            let predicate = chars_ok && trimmed != "default";
            prop_assert_eq!(validate_network_name(&s).is_ok(), predicate);
        }

        /// `validate_netmask` accepts iff the input parses as IPv4 AND
        /// its 32-bit representation is contiguous 1s followed by 0s.
        /// We re-derive the predicate independently using `Ipv4Addr`.
        #[test]
        fn validate_netmask_accepts_only_contiguous_masks(
            a in 0u8..=255, b in 0u8..=255, c in 0u8..=255, d in 0u8..=255,
        ) {
            let s = format!("{a}.{b}.{c}.{d}");
            let bits: u32 = ((a as u32) << 24) | ((b as u32) << 16) | ((c as u32) << 8) | (d as u32);
            // Contiguous mask ⇔ `bits == !((1 << zeros) - 1)` for some 0..=32.
            let zeros = bits.trailing_zeros();
            let contiguous = bits == !((1u32 << zeros).wrapping_sub(1)) || bits == 0xFFFFFFFF || bits == 0;
            // Adjust: if `zeros >= 32`, all bits zero. We allow that as a
            // degenerate "/0" mask which is itself contiguous.
            let result = validate_netmask(&s);
            prop_assert_eq!(result.is_ok(), contiguous);
        }

        /// `storage_backend_to_proto` is case-insensitive on the input
        /// and `Unspecified` for everything outside the known set.
        #[test]
        fn storage_backend_to_proto_known_set(s in ".{0,16}") {
            let normalized = s.trim().to_ascii_lowercase();
            let v = storage_backend_to_proto(&s);
            let expected = match normalized.as_str() {
                "filesystem" => controller_proto::StorageBackendType::Filesystem as i32,
                "lvm" => controller_proto::StorageBackendType::Lvm as i32,
                "zfs" => controller_proto::StorageBackendType::Zfs as i32,
                _ => controller_proto::StorageBackendType::Unspecified as i32,
            };
            prop_assert_eq!(v, expected);
        }

        /// `validate_storage_size_bytes` accepts iff `> 0` and is the
        /// identity on accepted inputs.
        #[test]
        fn validate_storage_size_bytes_accepts_only_positive(n in any::<i64>()) {
            let result = validate_storage_size_bytes(n);
            prop_assert_eq!(result.is_ok(), n > 0);
            if let Ok(v) = result {
                prop_assert_eq!(v, n);
            }
        }

        /// `derive_image_format` is case-insensitive on the URL extension
        /// and only ever returns `raw` or `qcow2`.
        #[test]
        fn derive_image_format_only_returns_known(s in ".{0,64}") {
            let f = derive_image_format(&s);
            prop_assert!(f == "raw" || f == "qcow2");
            let f2 = derive_image_format_from_path(&s);
            prop_assert!(f2 == "raw" || f2 == "qcow2");
        }
    }
}
