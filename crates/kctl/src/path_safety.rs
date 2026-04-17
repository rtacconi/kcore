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

/// Bounded model-checking proofs (Phase 2 — Kani).
///
/// Mirror of the controller-side proofs in
/// `crates/controller/src/path_safety.rs`. Both files implement the
/// same validators and both must hold the same security invariants.
///
/// To run:
/// ```text
/// cargo install --locked kani-verifier
/// cargo kani setup
/// cargo kani -p kcore-kctl
/// ```
#[cfg(kani)]
mod kani_proofs {
    use super::*;

    const MAX_INPUT_LEN: usize = 8;

    fn any_ascii_str(buf: &mut [u8; MAX_INPUT_LEN]) -> &str {
        let len: usize = kani::any();
        kani::assume(len <= MAX_INPUT_LEN);
        for slot in buf.iter_mut() {
            let b: u8 = kani::any();
            kani::assume(b < 128);
            *slot = b;
        }
        // SAFETY: every byte was constrained to < 128, so the slice is
        // valid UTF-8.
        std::str::from_utf8(&buf[..len]).unwrap()
    }

    #[kani::proof]
    #[kani::unwind(17)]
    fn dot_dot_check_never_panics() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let _ = path_segments_include_dot_dot(s);
    }

    #[kani::proof]
    #[kani::unwind(17)]
    fn assert_safe_path_never_panics() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let _ = assert_safe_path(s, "f");
    }

    /// **Soundness**: any input `assert_safe_path` accepts is
    /// non-empty, contains no NUL byte, and contains no `..` segment
    /// under either separator.
    #[kani::proof]
    #[kani::unwind(17)]
    fn assert_safe_path_acceptance_implies_safe() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        if assert_safe_path(s, "f").is_ok() {
            assert!(!s.is_empty());
            assert!(!s.contains('\0'));
            assert!(!path_segments_include_dot_dot(s));
        }
    }
}
