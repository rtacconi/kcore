//! Guards for path strings before filesystem access (traversal via `..`, NUL injection).

use anyhow::Result;

/// Rejects `..` in both `/` and `\` splits so strings like `file:../../x` cannot bypass
/// `std::path::Path` component parsing (which treats those as a single path segment).
pub fn path_segments_include_dot_dot(path: &str) -> bool {
    path.split(['/', '\\']).any(|segment| segment == "..")
}

pub fn assert_safe_path(path: &str, label: &str) -> Result<()> {
    if path.is_empty() {
        anyhow::bail!("{label} must not be empty");
    }
    if path.contains('\0') {
        anyhow::bail!("{label} must not contain NUL bytes");
    }
    if path_segments_include_dot_dot(path) {
        anyhow::bail!("{label} must not contain parent directory references ('..')");
    }
    Ok(())
}
