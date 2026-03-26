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
}
