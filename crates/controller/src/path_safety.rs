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

/// Bounded model-checking proofs (Phase 2 — Kani).
///
/// `path_segments_include_dot_dot` and `assert_safe_path` gate every
/// filesystem operation that takes operator-supplied input. We want
/// strong, exhaustive (within bounds) guarantees that they never panic
/// and never accept a string with a `..` segment under either
/// separator.
///
/// To run:
/// ```text
/// cargo install --locked kani-verifier
/// cargo kani setup
/// cargo kani -p kcore-controller
/// ```
#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Maximum input length we exhaustively check. Kani's runtime
    /// grows quickly with input length; 8 bytes is enough to cover
    /// every interesting alignment of `/`, `\`, `.`, and `\0`.
    const MAX_INPUT_LEN: usize = 8;

    /// Produce a non-deterministic ASCII string of length ≤
    /// `MAX_INPUT_LEN` for use in a Kani proof.
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

    /// `path_segments_include_dot_dot` never panics on any ASCII input
    /// up to `MAX_INPUT_LEN` bytes.
    #[kani::proof]
    #[kani::unwind(17)]
    fn dot_dot_check_never_panics() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let _ = path_segments_include_dot_dot(s);
    }

    /// `assert_safe_path` never panics on any ASCII input up to
    /// `MAX_INPUT_LEN` bytes.
    #[kani::proof]
    #[kani::unwind(17)]
    fn assert_safe_path_never_panics() {
        let mut buf = [0u8; MAX_INPUT_LEN];
        let s = any_ascii_str(&mut buf);
        let _ = assert_safe_path(s, "f");
    }

    /// **Soundness**: any input `assert_safe_path` accepts is
    /// non-empty, contains no NUL byte, and contains no `..` segment
    /// under either separator. This is the property the rest of the
    /// codebase relies on before passing the string to a filesystem
    /// operation.
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
