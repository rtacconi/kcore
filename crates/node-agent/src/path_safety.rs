//! Path-traversal sanitization helpers for tenant-supplied path segments.
//!
//! Rule of thumb: any path that comes from an RPC field — image name,
//! container name, volume handle, network bridge — MUST be passed through
//! one of these helpers before being joined with a trusted root or used
//! as a filesystem operand. `Path::starts_with` is **purely lexical** in
//! Rust; `/var/lib/kcore/images/../../etc/passwd` lexically begins with
//! `/var/lib/kcore/images` even though it resolves to `/etc/passwd`.
//!
//! Both helpers reject:
//!   * empty input,
//!   * any `..` segment,
//!   * leading `/` (for `validate_safe_segment`) — those are name-style
//!     fields that must not impersonate an absolute path,
//!   * `\0` bytes,
//!   * Windows-style separators (`\\`) — names must not include them.

/// Maximum length for a name-style segment (image name, container name,
/// volume handle, bridge name). Long enough for realistic operator names,
/// short enough to keep error messages and logs readable.
pub const MAX_SAFE_SEGMENT_LEN: usize = 200;

/// Validate a single path *segment* (no slashes, no `..`, no NULs). Returns
/// the trimmed segment on success.
///
/// Use this for fields that become the *last component* of a path, e.g.
/// image filename or container directory name. The caller is responsible
/// for joining the segment under a trusted root.
pub fn validate_safe_segment<'a>(name: &'a str, label: &str) -> Result<&'a str, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is required"));
    }
    if trimmed.len() > MAX_SAFE_SEGMENT_LEN {
        return Err(format!(
            "{label} is too long ({} bytes, max {})",
            trimmed.len(),
            MAX_SAFE_SEGMENT_LEN
        ));
    }
    if trimmed.contains('\0') {
        return Err(format!("{label} must not contain NUL bytes"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(format!("{label} must not contain path separators"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(format!("{label} must not be '.' or '..'"));
    }
    if trimmed.starts_with('-') {
        // Avoid being mistaken for a flag when forwarded to systemctl/zfs/etc.
        return Err(format!("{label} must not start with '-'"));
    }
    Ok(trimmed)
}

/// Validate that an absolute path (provided by an RPC caller) stays under
/// `root`. Lexical `starts_with` is not enough — `..` segments inside the
/// supplied path can escape `root` while still passing `starts_with`.
///
/// Returns the (untouched) `PathBuf` on success; rejects empty input,
/// non-absolute paths, NUL bytes, any `..` segment, and any path that
/// does not lexically begin with `root`.
pub fn validate_path_under_root(
    raw: &str,
    root: &std::path::Path,
    label: &str,
) -> Result<std::path::PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is required"));
    }
    if trimmed.contains('\0') {
        return Err(format!("{label} must not contain NUL bytes"));
    }
    let p = std::path::PathBuf::from(trimmed);
    if !p.is_absolute() {
        return Err(format!("{label} must be an absolute path"));
    }
    if !p.starts_with(root) {
        return Err(format!("{label} must be under {}", root.display()));
    }
    for component in p.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(format!("{label} must not contain '..' segments"));
        }
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn segment_rejects_empty_and_dots() {
        assert!(validate_safe_segment("", "name").is_err());
        assert!(validate_safe_segment("   ", "name").is_err());
        assert!(validate_safe_segment(".", "name").is_err());
        assert!(validate_safe_segment("..", "name").is_err());
    }

    #[test]
    fn segment_rejects_separators() {
        assert!(validate_safe_segment("foo/bar", "name").is_err());
        assert!(validate_safe_segment("foo\\bar", "name").is_err());
        assert!(validate_safe_segment("/abs", "name").is_err());
    }

    #[test]
    fn segment_rejects_nul() {
        assert!(validate_safe_segment("foo\0bar", "name").is_err());
    }

    #[test]
    fn segment_rejects_leading_dash() {
        // Defends against `systemctl start -unit-that-looks-like-a-flag`.
        assert!(validate_safe_segment("-foo", "name").is_err());
    }

    #[test]
    fn segment_accepts_normal_names() {
        assert_eq!(
            validate_safe_segment("debian-12.qcow2", "image").unwrap(),
            "debian-12.qcow2"
        );
        assert_eq!(
            validate_safe_segment("  web-01  ", "name").unwrap(),
            "web-01"
        );
    }

    #[test]
    fn segment_rejects_overlong() {
        let huge = "a".repeat(MAX_SAFE_SEGMENT_LEN + 1);
        assert!(validate_safe_segment(&huge, "name").is_err());
    }

    #[test]
    fn under_root_rejects_relative_or_outside() {
        let root = Path::new("/var/lib/kcore/images");
        assert!(validate_path_under_root("relative.raw", root, "p").is_err());
        assert!(validate_path_under_root("/etc/passwd", root, "p").is_err());
        assert!(validate_path_under_root("", root, "p").is_err());
        assert!(validate_path_under_root("\0", root, "p").is_err());
    }

    #[test]
    fn under_root_rejects_dotdot_traversal_even_when_starts_with_root() {
        // CRITICAL: bare `starts_with` would PASS this check because
        // the lexical prefix is /var/lib/kcore/images. The fix is to
        // walk components and forbid ParentDir.
        let root = Path::new("/var/lib/kcore/images");
        let bad = "/var/lib/kcore/images/../../../etc/passwd";
        let err = validate_path_under_root(bad, root, "image_path")
            .expect_err("must reject .. traversal");
        assert!(err.contains(".."), "should mention dot-dot, got: {err}");
    }

    #[test]
    fn under_root_accepts_clean_path() {
        let root = Path::new("/var/lib/kcore/images");
        let p = validate_path_under_root("/var/lib/kcore/images/debian.qcow2", root, "p")
            .expect("clean path");
        assert_eq!(p, Path::new("/var/lib/kcore/images/debian.qcow2"));
    }
}
