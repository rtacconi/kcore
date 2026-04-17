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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_dot_dot_in_unix_paths() {
        assert!(path_segments_include_dot_dot("../etc/passwd"));
        assert!(path_segments_include_dot_dot("foo/../bar"));
        assert!(path_segments_include_dot_dot("/abs/../sneaky"));
        assert!(path_segments_include_dot_dot(".."));
    }

    #[test]
    fn detects_dot_dot_in_windows_style_paths() {
        // We split on both `/` and `\` so a string like `file:..\..\x`
        // can't bypass the segment check by using backslashes.
        assert!(path_segments_include_dot_dot("foo\\..\\bar"));
        assert!(path_segments_include_dot_dot("..\\windows"));
    }

    #[test]
    fn allows_clean_paths() {
        assert!(!path_segments_include_dot_dot(""));
        assert!(!path_segments_include_dot_dot("foo/bar"));
        assert!(
            !path_segments_include_dot_dot("..foo"),
            "embedded .. is not a segment"
        );
        assert!(!path_segments_include_dot_dot("foo..bar"));
        assert!(!path_segments_include_dot_dot("...")); // three dots is a single segment
    }

    #[test]
    fn assert_safe_path_rejects_empty() {
        let err = assert_safe_path("", "p").expect_err("empty must fail");
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn assert_safe_path_rejects_nul() {
        let err = assert_safe_path("foo\0bar", "p").expect_err("NUL must fail");
        assert!(err.to_string().contains("NUL"));
    }

    #[test]
    fn assert_safe_path_rejects_traversal_with_both_separators() {
        assert!(assert_safe_path("a/../b", "p").is_err());
        assert!(assert_safe_path("a\\..\\b", "p").is_err());
    }

    #[test]
    fn assert_safe_path_accepts_clean_paths() {
        assert!(assert_safe_path("foo/bar/baz.txt", "p").is_ok());
        assert!(assert_safe_path("/absolute/path", "p").is_ok());
        assert!(assert_safe_path("file..with..dots", "p").is_ok());
    }
}
