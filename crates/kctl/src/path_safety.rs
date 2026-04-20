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

/// Property-based tests (Phase 1).
///
/// Path-safety helpers are pure validators that gate every filesystem
/// operation that takes operator-supplied input. We want strong guarantees
/// against panics and against false negatives on the traversal check, so
/// proptest is a much better fit than the handful of example tests above.
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// Both validators must terminate without panicking on **any**
        /// string up to a generous length, including non-UTF-8-looking
        /// content, embedded NULs, and arbitrary separator soup.
        #[test]
        fn validators_never_panic(s in ".{0,64}") {
            let _ = path_segments_include_dot_dot(&s);
            let _ = assert_safe_path(&s, "field");
        }

        /// Soundness of `path_segments_include_dot_dot`:
        /// if any unix or windows separator-delimited segment of the
        /// input is exactly `..`, the function must return `true`.
        /// (This is the contract every caller relies on.)
        #[test]
        fn dot_dot_is_detected_when_present(
            prefix in "[a-zA-Z0-9_./\\\\]{0,16}",
            suffix in "[a-zA-Z0-9_./\\\\]{0,16}",
            sep in prop::sample::select(vec!["/", "\\"]),
        ) {
            let s = format!("{prefix}{sep}..{sep}{suffix}");
            prop_assert!(
                path_segments_include_dot_dot(&s),
                "must detect .. in {s:?}"
            );
        }

        /// Conversely, a path built from only safe segments
        /// (no `.` / `..`, no NUL, no separators in the segments
        /// themselves) must NOT trip the traversal detector.
        #[test]
        fn safe_segments_never_trigger_dot_dot_detector(
            segments in proptest::collection::vec("[a-zA-Z0-9_-]{1,8}", 1..6),
            sep in prop::sample::select(vec!["/", "\\"]),
        ) {
            // Filter out the (legal but irrelevant) case where the regex
            // happened to produce only digits/letters that equal "..":
            // that's impossible with our character class and 1..=8 length.
            let joined = segments.join(sep);
            prop_assert!(
                !path_segments_include_dot_dot(&joined),
                "false positive on safe path {joined:?}"
            );
        }

        /// `assert_safe_path` invariant: if the input contains either a
        /// NUL byte OR a `..` segment OR is empty, the result MUST be
        /// `Err`. This is the property the rest of the codebase relies on
        /// before using the string as a filesystem path.
        #[test]
        fn assert_safe_path_rejects_unsafe_inputs(s in ".{0,64}") {
            let must_reject = s.is_empty()
                || s.contains('\0')
                || path_segments_include_dot_dot(&s);
            let result = assert_safe_path(&s, "field");
            if must_reject {
                prop_assert!(
                    result.is_err(),
                    "assert_safe_path({s:?}) accepted unsafe input"
                );
            }
        }
    }
}
