//! Pure, std-only sanitizers for operator-supplied input that flows
//! into a Nix expression or a filesystem operation.
//!
//! Every function in this crate is a *gate* — its callers depend on
//! the post-conditions being upheld for memory safety, sandbox
//! integrity, or to stop arbitrary Nix evaluation. The
//! security-critical post-conditions are proved with Kani in
//! `mod kani_proofs` below.
//!
//! This crate intentionally has **no non-std dependencies** so that
//! `cargo kani -p kcore-sanitize` only needs to compile this single
//! file, keeping the formal-checks CI gate fast and reliable.

// =============================================================
// Nix string-literal escaping
// =============================================================

/// Escape a string for safe inclusion inside a Nix double-quoted
/// string literal (`"…"`). Escapes `\`, `"`, and the `${`
/// interpolation marker.
pub fn nix_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' if chars.peek() == Some(&'{') => {
                out.push_str("\\${");
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

/// Strip a Nix attribute key to only safe characters
/// (alphanumeric, dash, underscore). Every disallowed input
/// character is replaced by a single `-`, so character count is
/// preserved.
pub fn sanitize_nix_attr_key(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

// =============================================================
// Path traversal / NUL injection guards
// =============================================================

/// Rejects `..` in both `/` and `\` splits so strings like
/// `file:../../x` cannot bypass `std::path::Path` component parsing
/// (which treats those as a single path segment).
///
/// Implemented as a flat byte loop rather than `path.split([...])
/// .any(...)`. Functionally equivalent (the separator chars `/` and
/// `\` are single-byte ASCII, so byte-level splitting matches
/// `str::split` over a UTF-8 string), but the byte loop generates a
/// dramatically smaller symbolic formula for Kani: the iterator-
/// over-`Pattern` machinery in `core::str::pattern` produced ~9 min
/// CBMC runs on a 4-byte input, while this loop completes the same
/// harness in seconds.
pub fn path_segments_include_dot_dot(path: &str) -> bool {
    let bytes = path.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    let n = bytes.len();
    while i <= n {
        let at_boundary = i == n || bytes[i] == b'/' || bytes[i] == b'\\';
        if at_boundary {
            if i - start == 2 && bytes[start] == b'.' && bytes[start + 1] == b'.' {
                return true;
            }
            start = i + 1;
        }
        i += 1;
    }
    false
}

/// Reasons `assert_safe_path` may reject a string. The caller is
/// expected to convert this into its own error type with a
/// human-readable label (see the wrapping `assert_safe_path`
/// functions in `crates/controller/src/path_safety.rs` and
/// `crates/kctl/src/path_safety.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafePathError {
    Empty,
    ContainsNul,
    ContainsParentDir,
}

/// Validate a free-form path string. The label/formatting of the
/// error message is left to the caller so this crate has no
/// dependency on `anyhow`.
pub fn assert_safe_path(path: &str) -> Result<(), SafePathError> {
    if path.is_empty() {
        return Err(SafePathError::Empty);
    }
    if path.contains('\0') {
        return Err(SafePathError::ContainsNul);
    }
    if path_segments_include_dot_dot(path) {
        return Err(SafePathError::ContainsParentDir);
    }
    Ok(())
}

// =============================================================
// Path *segment* (last-component) guards used by node-agent for
// tenant-supplied names (image, container, volume, bridge).
// =============================================================

/// Maximum length for a name-style segment (image name, container
/// name, volume handle, bridge name). Long enough for realistic
/// operator names, short enough to keep error messages and logs
/// readable.
pub const MAX_SAFE_SEGMENT_LEN: usize = 200;

/// Reasons `validate_safe_segment` may reject a string. The caller
/// formats this into its own user-facing error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafeSegmentError {
    Empty,
    TooLong { actual: usize, max: usize },
    ContainsNul,
    ContainsSeparator,
    DotOrDotDot,
    LeadingDash,
}

/// Validate a single path *segment* (no slashes, no `..`, no NULs,
/// no leading `-`). Returns the trimmed segment on success.
///
/// Use this for fields that become the *last component* of a path,
/// e.g. image filename or container directory name. The caller is
/// responsible for joining the segment under a trusted root.
pub fn validate_safe_segment(name: &str) -> Result<&str, SafeSegmentError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(SafeSegmentError::Empty);
    }
    if trimmed.len() > MAX_SAFE_SEGMENT_LEN {
        return Err(SafeSegmentError::TooLong {
            actual: trimmed.len(),
            max: MAX_SAFE_SEGMENT_LEN,
        });
    }
    if trimmed.contains('\0') {
        return Err(SafeSegmentError::ContainsNul);
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(SafeSegmentError::ContainsSeparator);
    }
    if trimmed == "." || trimmed == ".." {
        return Err(SafeSegmentError::DotOrDotDot);
    }
    if trimmed.starts_with('-') {
        // Avoid being mistaken for a flag when forwarded to
        // systemctl/zfs/etc.
        return Err(SafeSegmentError::LeadingDash);
    }
    Ok(trimmed)
}

/// Reasons `validate_path_under_root` may reject a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafeRootedPathError {
    Empty,
    ContainsNul,
    NotAbsolute,
    NotUnderRoot,
    ContainsParentDir,
}

/// Validate that an absolute path (provided by an RPC caller) stays
/// under `root`. Lexical `starts_with` is not enough — `..`
/// segments inside the supplied path can escape `root` while still
/// passing `starts_with`.
///
/// Returns the (untouched) `PathBuf` on success.
pub fn validate_path_under_root(
    raw: &str,
    root: &std::path::Path,
) -> Result<std::path::PathBuf, SafeRootedPathError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SafeRootedPathError::Empty);
    }
    if trimmed.contains('\0') {
        return Err(SafeRootedPathError::ContainsNul);
    }
    let p = std::path::PathBuf::from(trimmed);
    if !p.is_absolute() {
        return Err(SafeRootedPathError::NotAbsolute);
    }
    if !p.starts_with(root) {
        return Err(SafeRootedPathError::NotUnderRoot);
    }
    for component in p.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(SafeRootedPathError::ContainsParentDir);
        }
    }
    Ok(p)
}

// =============================================================
// Unit tests (run on every `cargo test`, no Kani required)
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ---- nix_escape ----

    #[test]
    fn nix_escape_handles_quotes_backslashes_and_interpolation() {
        assert_eq!(nix_escape(r#"a"b"#), r#"a\"b"#);
        assert_eq!(nix_escape(r"a\b"), r"a\\b");
        assert_eq!(nix_escape("a${b}"), "a\\${b}");
        assert_eq!(nix_escape("plain"), "plain");
    }

    /// Regression: previously `nix_escape` iterated over `&[u8]` and
    /// reconstructed each byte with `bytes[i] as char`, which corrupts
    /// every multi-byte UTF-8 scalar (e.g. "café" → "cafÃ©", "🦀" →
    /// four mojibake control bytes). Operator-supplied identifiers
    /// (VM names, security-group names, datacentre labels) are valid
    /// `&str` so they may legitimately contain non-ASCII; the Kani
    /// proofs only feed ASCII input via `any_ascii_str`, so this case
    /// must be covered by an explicit unit test.
    #[test]
    fn nix_escape_preserves_non_ascii_utf8() {
        assert_eq!(nix_escape("café"), "café");
        assert_eq!(nix_escape("naïve"), "naïve");
        assert_eq!(nix_escape("café${x}"), "café\\${x}");
        assert_eq!(nix_escape("🦀"), "🦀");
        assert_eq!(nix_escape("🦀${rust}"), "🦀\\${rust}");
        assert_eq!(nix_escape("日本語"), "日本語");
    }

    // ---- sanitize_nix_attr_key ----

    #[test]
    fn sanitize_nix_attr_key_replaces_unsafe_chars_with_dash() {
        assert_eq!(sanitize_nix_attr_key("foo bar"), "foo-bar");
        assert_eq!(sanitize_nix_attr_key("a/b\\c"), "a-b-c");
        assert_eq!(sanitize_nix_attr_key("ok_name-1"), "ok_name-1");
    }

    #[test]
    fn sanitize_nix_attr_key_preserves_char_count() {
        for s in ["", "a", "ab", "a/b", "$$$"] {
            assert_eq!(sanitize_nix_attr_key(s).chars().count(), s.chars().count());
        }
    }

    // ---- path_segments_include_dot_dot ----

    #[test]
    fn detects_dot_dot_in_unix_paths() {
        assert!(path_segments_include_dot_dot("../etc/passwd"));
        assert!(path_segments_include_dot_dot("foo/../bar"));
        assert!(path_segments_include_dot_dot("/abs/../sneaky"));
        assert!(path_segments_include_dot_dot(".."));
    }

    #[test]
    fn detects_dot_dot_in_windows_style_paths() {
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
        assert!(!path_segments_include_dot_dot("..."));
    }

    // ---- assert_safe_path ----

    #[test]
    fn assert_safe_path_classifies_errors() {
        assert_eq!(assert_safe_path(""), Err(SafePathError::Empty));
        assert_eq!(assert_safe_path("a\0b"), Err(SafePathError::ContainsNul));
        assert_eq!(
            assert_safe_path("a/../b"),
            Err(SafePathError::ContainsParentDir)
        );
        assert_eq!(
            assert_safe_path("a\\..\\b"),
            Err(SafePathError::ContainsParentDir)
        );
        assert!(assert_safe_path("foo/bar/baz.txt").is_ok());
        assert!(assert_safe_path("/absolute/path").is_ok());
        assert!(assert_safe_path("file..with..dots").is_ok());
    }

    // ---- validate_safe_segment ----

    #[test]
    fn segment_classifies_errors() {
        assert_eq!(validate_safe_segment(""), Err(SafeSegmentError::Empty));
        assert_eq!(validate_safe_segment("   "), Err(SafeSegmentError::Empty));
        assert_eq!(
            validate_safe_segment("."),
            Err(SafeSegmentError::DotOrDotDot)
        );
        assert_eq!(
            validate_safe_segment(".."),
            Err(SafeSegmentError::DotOrDotDot)
        );
        assert_eq!(
            validate_safe_segment("foo/bar"),
            Err(SafeSegmentError::ContainsSeparator)
        );
        assert_eq!(
            validate_safe_segment("foo\\bar"),
            Err(SafeSegmentError::ContainsSeparator)
        );
        assert_eq!(
            validate_safe_segment("/abs"),
            Err(SafeSegmentError::ContainsSeparator)
        );
        assert_eq!(
            validate_safe_segment("foo\0bar"),
            Err(SafeSegmentError::ContainsNul)
        );
        assert_eq!(
            validate_safe_segment("-foo"),
            Err(SafeSegmentError::LeadingDash)
        );
    }

    #[test]
    fn segment_accepts_normal_names() {
        assert_eq!(
            validate_safe_segment("debian-12.qcow2").unwrap(),
            "debian-12.qcow2"
        );
        assert_eq!(validate_safe_segment("  web-01  ").unwrap(), "web-01");
    }

    #[test]
    fn segment_rejects_overlong() {
        let huge = "a".repeat(MAX_SAFE_SEGMENT_LEN + 1);
        assert!(matches!(
            validate_safe_segment(&huge),
            Err(SafeSegmentError::TooLong { .. })
        ));
    }

    // ---- validate_path_under_root ----

    #[test]
    fn under_root_classifies_errors() {
        let root = Path::new("/var/lib/kcore/images");
        assert_eq!(
            validate_path_under_root("relative.raw", root),
            Err(SafeRootedPathError::NotAbsolute)
        );
        assert_eq!(
            validate_path_under_root("/etc/passwd", root),
            Err(SafeRootedPathError::NotUnderRoot)
        );
        assert_eq!(
            validate_path_under_root("", root),
            Err(SafeRootedPathError::Empty)
        );
        assert_eq!(
            validate_path_under_root("\0", root),
            Err(SafeRootedPathError::ContainsNul)
        );
    }

    #[test]
    fn under_root_rejects_dotdot_traversal_even_when_starts_with_root() {
        let root = Path::new("/var/lib/kcore/images");
        let bad = "/var/lib/kcore/images/../../../etc/passwd";
        assert_eq!(
            validate_path_under_root(bad, root),
            Err(SafeRootedPathError::ContainsParentDir)
        );
    }

    #[test]
    fn under_root_accepts_clean_path() {
        let root = Path::new("/var/lib/kcore/images");
        let p = validate_path_under_root("/var/lib/kcore/images/debian.qcow2", root)
            .expect("clean path");
        assert_eq!(p, Path::new("/var/lib/kcore/images/debian.qcow2"));
    }
}

// =============================================================
// Bounded model-checking proofs (Phase 2 — Kani)
// =============================================================

/// Bounded model-checking proofs.
///
/// Run with:
///
/// ```text
/// cargo install --locked kani-verifier
/// cargo kani setup
/// cargo kani -p kcore-sanitize
/// ```
///
/// Bounds (`MAX_INPUT_LEN`) are kept small so each harness finishes
/// in seconds on the GitHub Actions runner. They are large enough
/// to cover every interesting alignment of the security-relevant
/// tokens (`\`, `"`, `${`, `/`, `\`, `.`, `\0`, leading `-`).
#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Maximum input length used by every Kani harness in this
    /// module. Kani's runtime grows quickly with this bound; 4
    /// bytes is enough to cover every interesting alignment of the
    /// security-relevant tokens above.
    const MAX_INPUT_LEN: usize = 4;

    /// Produce a non-deterministic ASCII string of length ≤
    /// `MAX_INPUT_LEN` for use in a Kani proof. Restricting to
    /// ASCII keeps the model small while still covering every byte
    /// that can appear in any of the escape alphabets we care
    /// about.
    fn any_ascii_str(buf: &mut [u8; MAX_INPUT_LEN]) -> &str {
        let len: usize = kani::any();
        kani::assume(len <= MAX_INPUT_LEN);
        for slot in buf.iter_mut() {
            let b: u8 = kani::any();
            kani::assume(b < 128);
            *slot = b;
        }
        // SAFETY: every byte was constrained to < 128, so the
        // slice is valid UTF-8.
        std::str::from_utf8(&buf[..len]).unwrap()
    }

    /// Returns true iff `s`, when wrapped in `"…"`, is safe to
    /// embed in a Nix double-quoted string literal: no unescaped
    /// `"`, no unescaped `\`, and no unescaped `${` interpolation
    /// marker.
    fn is_safely_escaped(s: &str) -> bool {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => {
                    if i + 1 >= bytes.len() {
                        return false;
                    }
                    i += 2;
                    continue;
                }
                b'"' => return false,
                b'$' if bytes.get(i + 1) == Some(&b'{') => return false,
                _ => i += 1,
            }
        }
        true
    }

    // ---- nix_escape ----

    #[kani::proof]
    #[kani::unwind(9)]
    fn nix_escape_never_panics() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let _ = nix_escape(s);
    }

    /// **Soundness**: `nix_escape` output is always safe to embed
    /// in a Nix double-quoted string literal.
    #[kani::proof]
    #[kani::unwind(9)]
    fn nix_escape_output_is_always_safe() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let escaped = nix_escape(s);
        assert!(is_safely_escaped(&escaped));
    }

    // ---- sanitize_nix_attr_key ----

    #[kani::proof]
    #[kani::unwind(9)]
    fn sanitize_nix_attr_key_preserves_char_count() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let out = sanitize_nix_attr_key(s);
        assert!(out.chars().count() == s.chars().count());
    }

    #[kani::proof]
    #[kani::unwind(9)]
    fn sanitize_nix_attr_key_charset() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let out = sanitize_nix_attr_key(s);
        for c in out.chars() {
            assert!(c.is_ascii_alphanumeric() || c == '-' || c == '_');
        }
    }

    // ---- path_segments_include_dot_dot / assert_safe_path ----
    //
    // These three harnesses exercise the byte-loop path scanner.
    // With `MAX_INPUT_LEN = 4` the scanning loop iterates at most 5
    // times (one extra for the trailing `i == n` boundary case),
    // so `unwind(6)` is sufficient. CBMC will still hard-fail with
    // an unwinding-assertion violation if a future bound increase
    // outgrows this, so this is safe to tighten.

    #[kani::proof]
    #[kani::unwind(6)]
    fn dot_dot_check_never_panics() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let _ = path_segments_include_dot_dot(s);
    }

    #[kani::proof]
    #[kani::unwind(6)]
    fn assert_safe_path_never_panics() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let _ = assert_safe_path(s);
    }

    /// **Soundness**: any input `assert_safe_path` accepts is
    /// non-empty, contains no NUL byte, and contains no `..`
    /// segment under either separator.
    #[kani::proof]
    #[kani::unwind(6)]
    fn assert_safe_path_acceptance_implies_safe() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        if assert_safe_path(s).is_ok() {
            assert!(!s.is_empty());
            assert!(!s.contains('\0'));
            assert!(!path_segments_include_dot_dot(s));
        }
    }

    // ---- validate_safe_segment ----

    #[kani::proof]
    #[kani::unwind(9)]
    fn segment_validation_never_panics() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let _ = validate_safe_segment(s);
    }

    /// **Soundness**: any segment `validate_safe_segment` accepts
    /// is non-empty after trimming, contains no NUL byte, no path
    /// separator (`/` or `\`), is not `.` or `..`, and does not
    /// start with `-`.
    #[kani::proof]
    #[kani::unwind(9)]
    fn segment_acceptance_implies_safe() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        if let Ok(out) = validate_safe_segment(s) {
            assert!(!out.is_empty());
            assert!(!out.contains('\0'));
            assert!(!out.contains('/'));
            assert!(!out.contains('\\'));
            assert!(out != "." && out != "..");
            assert!(!out.starts_with('-'));
            assert!(out.len() <= MAX_SAFE_SEGMENT_LEN);
        }
    }
}
