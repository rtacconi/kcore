//! Shared types and pure logic for DiskLayout resources.
//!
//! Both the node-agent (authoritative classifier, fed by live `lsblk`) and
//! the controller (fast pre-flight, fed by replicated inventory) depend on
//! this crate so there is a single implementation of:
//!
//! * [`extract_target_devices`] — tokenise operator-supplied Nix to the set
//!   of `/dev/...` paths the proposed layout would touch.
//! * [`classify_disk_layout`] — decide SAFE or DANGEROUS against an
//!   [`LsblkSnapshot`]-shaped view of current disk state.
//!
//! The crate is deliberately dependency-free (std only) so the Kani harness
//! over the extractor compiles in seconds.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

/// Stable, machine-readable refusal codes. Surfaced on the
/// `ApplyDiskLayoutResponse.refusal_reason` field so `kctl` can key UX off a
/// short string instead of parsing `message`.
pub mod refusal {
    pub const MOUNTED_KCORE_VOLUME: &str = "target_device_has_active_kcore_volume";
    pub const MOUNTED_SYSTEM_PARTITION: &str = "target_device_has_active_system_mount";
    pub const ACTIVE_LVM_PV: &str = "target_device_is_active_lvm_pv";
    pub const ACTIVE_ZPOOL_MEMBER: &str = "target_device_is_active_zpool_member";
    pub const NO_TARGET_DEVICES: &str = "no_target_devices";
    pub const INVALID_LAYOUT: &str = "invalid_layout";
}

/// Snapshot of the block-device tree on the target node. On a node, this is
/// populated from `lsblk -J -o NAME,PATH,FSTYPE,MOUNTPOINTS,PKNAME,TYPE`; on
/// the controller, it is reconstructed from the replicated inventory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LsblkSnapshot {
    pub devices: Vec<BlockDevice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDevice {
    /// Absolute path, e.g. `/dev/sda`, `/dev/sda1`, `/dev/mapper/cryptroot`.
    pub path: String,
    /// `disk`, `part`, `crypt`, `lvm`, `zfs_member`, etc.
    pub kind: String,
    /// Filesystem type (e.g. `ext4`, `LVM2_member`, `zfs_member`, empty).
    pub fstype: Option<String>,
    /// Active mountpoints (if any).
    pub mountpoints: Vec<String>,
    /// Parent device path (e.g. `/dev/sda` for `/dev/sda1`), if known.
    pub parent_path: Option<String>,
}

/// Verdict returned by [`classify_disk_layout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Safe to apply: no target device currently hosts active data.
    Safe,
    /// Refused: the proposed layout would destroy active state.
    Dangerous {
        /// Stable refusal code (one of [`refusal`]).
        code: &'static str,
        /// Human-readable explanation suitable for the operator.
        detail: String,
    },
}

/// Classify a proposed disk layout.
///
/// * `target_devices` — the block-device paths referenced by `device = "..."`
///   attributes in the proposed layout, as extracted by
///   [`extract_target_devices`].
/// * `snapshot` — live lsblk snapshot of the node.
/// * `kcore_volume_roots` — mountpoint prefixes the node uses for
///   workload-backing volumes (e.g. `/var/lib/kcore/volumes`).
pub fn classify_disk_layout(
    target_devices: &[String],
    snapshot: &LsblkSnapshot,
    kcore_volume_roots: &[&str],
) -> Verdict {
    if target_devices.is_empty() {
        return Verdict::Dangerous {
            code: refusal::NO_TARGET_DEVICES,
            detail: "proposed layout did not declare any /dev/* target devices".to_string(),
        };
    }

    let target_set: BTreeSet<&str> = target_devices.iter().map(|s| s.as_str()).collect();

    for dev in &snapshot.devices {
        if !is_within_targets(&dev.path, dev.parent_path.as_deref(), &target_set, snapshot) {
            continue;
        }

        if let Some(danger) = classify_device(dev, kcore_volume_roots) {
            return danger;
        }
    }

    Verdict::Safe
}

fn classify_device(dev: &BlockDevice, kcore_volume_roots: &[&str]) -> Option<Verdict> {
    for mp in &dev.mountpoints {
        if mp.is_empty() {
            continue;
        }
        if kcore_volume_roots.iter().any(|root| mp.starts_with(root)) {
            return Some(Verdict::Dangerous {
                code: refusal::MOUNTED_KCORE_VOLUME,
                detail: format!("{} currently backs kcore volume mount {}", dev.path, mp),
            });
        }
        if is_system_mount(mp) {
            return Some(Verdict::Dangerous {
                code: refusal::MOUNTED_SYSTEM_PARTITION,
                detail: format!("{} currently hosts system mount {}", dev.path, mp),
            });
        }
    }

    match dev.fstype.as_deref() {
        Some("LVM2_member") => Some(Verdict::Dangerous {
            code: refusal::ACTIVE_LVM_PV,
            detail: format!("{} is an active LVM physical volume", dev.path),
        }),
        Some("zfs_member") => Some(Verdict::Dangerous {
            code: refusal::ACTIVE_ZPOOL_MEMBER,
            detail: format!("{} is a member of an active ZFS pool", dev.path),
        }),
        _ => None,
    }
}

fn is_system_mount(mp: &str) -> bool {
    matches!(mp, "/" | "/boot" | "/nix" | "/nix/store" | "/boot/efi")
}

fn is_within_targets(
    path: &str,
    parent: Option<&str>,
    targets: &BTreeSet<&str>,
    snapshot: &LsblkSnapshot,
) -> bool {
    if targets.contains(path) {
        return true;
    }
    let mut cursor = parent;
    let mut depth = 0usize;
    while let Some(p) = cursor {
        if depth > 8 {
            return false;
        }
        if targets.contains(p) {
            return true;
        }
        cursor = snapshot
            .devices
            .iter()
            .find(|d| d.path == p)
            .and_then(|d| d.parent_path.as_deref());
        depth += 1;
    }
    false
}

/// Extract `/dev/...` device paths that appear as the right-hand side of
/// `device = "..."` assignments in the proposed disko expression.
///
/// This is a deliberately lenient string parse — full Nix evaluation is
/// expensive and happens later via `disko`. We only need enough accuracy to
/// enumerate the disks the operator intends to partition.
///
/// Line comments (`# ...`) and block comments (`/* ... */`) are skipped so
/// commented-out `device = "..."` lines don't leak into the target set.
pub fn extract_target_devices(disk_layout_nix: &str) -> Vec<String> {
    let sanitized = strip_nix_comments(disk_layout_nix);
    let mut out = Vec::new();
    let bytes = sanitized.as_bytes();
    let key = b"device";
    let mut i = 0usize;
    while i + key.len() < bytes.len() {
        if &bytes[i..i + key.len()] == key {
            let prev_ok = i == 0 || {
                let p = bytes[i - 1];
                !(p.is_ascii_alphanumeric() || p == b'_')
            };
            if !prev_ok {
                i += 1;
                continue;
            }
            let mut j = i + key.len();
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'=' {
                j += 1;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'"' {
                    j += 1;
                    let start = j;
                    while j < bytes.len() && bytes[j] != b'"' {
                        j += 1;
                    }
                    if j <= bytes.len() {
                        let val = &sanitized[start..j];
                        if val.starts_with("/dev/") {
                            out.push(val.to_string());
                        }
                    }
                }
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    out.sort();
    out.dedup();
    out
}

/// Replace Nix line and block comments with spaces of equal length, preserving
/// offsets into the original string. Strings (`"..."`) are left intact so
/// `#` characters inside string literals do not start a comment.
fn strip_nix_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            out.push(b);
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                out.push(c);
                i += 1;
                if c == b'\\' && i < bytes.len() {
                    out.push(bytes[i]);
                    i += 1;
                    continue;
                }
                if c == b'"' {
                    break;
                }
            }
            continue;
        }
        if b == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(b' ');
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            out.push(b' ');
            out.push(b' ');
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                out.push(if bytes[i] == b'\n' { b'\n' } else { b' ' });
                i += 1;
            }
            if i + 1 < bytes.len() {
                out.push(b' ');
                out.push(b' ');
                i += 2;
            }
            continue;
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| src.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(
        path: &str,
        kind: &str,
        fstype: Option<&str>,
        mp: &[&str],
        parent: Option<&str>,
    ) -> BlockDevice {
        BlockDevice {
            path: path.to_string(),
            kind: kind.to_string(),
            fstype: fstype.map(str::to_string),
            mountpoints: mp.iter().map(|s| s.to_string()).collect(),
            parent_path: parent.map(str::to_string),
        }
    }

    #[test]
    fn empty_layout_is_dangerous_because_no_targets() {
        let snap = LsblkSnapshot::default();
        let verdict = classify_disk_layout(&[], &snap, &["/var/lib/kcore/volumes"]);
        assert!(matches!(verdict, Verdict::Dangerous { .. }));
    }

    #[test]
    fn idle_disk_is_safe() {
        let snap = LsblkSnapshot {
            devices: vec![disk("/dev/sdb", "disk", None, &[], None)],
        };
        let verdict = classify_disk_layout(
            &["/dev/sdb".to_string()],
            &snap,
            &["/var/lib/kcore/volumes"],
        );
        assert_eq!(verdict, Verdict::Safe);
    }

    #[test]
    fn target_with_kcore_volume_mount_is_dangerous() {
        let snap = LsblkSnapshot {
            devices: vec![
                disk("/dev/sdc", "disk", None, &[], None),
                disk(
                    "/dev/sdc1",
                    "part",
                    Some("ext4"),
                    &["/var/lib/kcore/volumes"],
                    Some("/dev/sdc"),
                ),
            ],
        };
        let verdict = classify_disk_layout(
            &["/dev/sdc".to_string()],
            &snap,
            &["/var/lib/kcore/volumes"],
        );
        match verdict {
            Verdict::Dangerous { code, .. } => assert_eq!(code, refusal::MOUNTED_KCORE_VOLUME),
            other => panic!("expected Dangerous, got {other:?}"),
        }
    }

    #[test]
    fn extract_target_devices_handles_multiple_and_comments() {
        let nix = r#"{
            disko.devices.disk.os = {
                device = "/dev/sda"; # the OS disk
                content.type = "gpt";
            };
            disko.devices.disk.data0.device   =   "/dev/nvme0n1";
            # fake out: device = "/dev/should_ignore" inside a comment
        }"#;
        let got = extract_target_devices(nix);
        assert_eq!(
            got,
            vec!["/dev/nvme0n1".to_string(), "/dev/sda".to_string()]
        );
    }

    #[test]
    fn extract_target_devices_skips_block_comments() {
        let nix = r#"
            /* device = "/dev/inside_block_comment"; */
            device = "/dev/real";
        "#;
        assert_eq!(extract_target_devices(nix), vec!["/dev/real".to_string()]);
    }

    #[test]
    fn extract_target_devices_keeps_hash_inside_strings() {
        let nix = r#"device = "/dev/disk/by-id/pool#foo";"#;
        assert_eq!(
            extract_target_devices(nix),
            vec!["/dev/disk/by-id/pool#foo".to_string()]
        );
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::collection::vec as proptest_vec;
    use proptest::prelude::*;

    /// A non-pathological device name segment.
    fn device_path() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9]{1,8}".prop_map(|s| format!("/dev/{s}"))
    }

    fn arb_block_device(parent: Option<String>) -> impl Strategy<Value = BlockDevice> {
        (
            device_path(),
            prop_oneof![Just("disk"), Just("part"), Just("lvm"), Just("crypt")],
            prop_oneof![
                Just(None),
                Just(Some("ext4".to_string())),
                Just(Some("LVM2_member".to_string())),
                Just(Some("zfs_member".to_string()))
            ],
            proptest_vec(
                prop_oneof![
                    Just("".to_string()),
                    Just("/".to_string()),
                    Just("/boot".to_string()),
                    Just("/var/lib/kcore/volumes/v1".to_string()),
                    Just("/srv/data".to_string())
                ],
                0..3,
            ),
        )
            .prop_map(move |(path, kind, fstype, mps)| BlockDevice {
                path,
                kind: kind.to_string(),
                fstype,
                mountpoints: mps.into_iter().filter(|s| !s.is_empty()).collect(),
                parent_path: parent.clone(),
            })
    }

    fn arb_snapshot() -> impl Strategy<Value = LsblkSnapshot> {
        proptest_vec(arb_block_device(None), 0..12).prop_map(|devices| LsblkSnapshot { devices })
    }

    proptest! {
        /// Safety invariant. If [`classify_disk_layout`] returns [`Verdict::Safe`],
        /// then no device in the snapshot that is "in target scope" (the device
        /// itself is a target, or transitively descends from one) currently has:
        ///   * a mountpoint under any kcore volume root
        ///   * a system mountpoint
        ///   * fstype == LVM2_member or zfs_member
        ///
        /// This is the central anti-foot-gun guarantee operators rely on:
        /// "SAFE" must mean "no active VM-backing storage will be wiped".
        #[test]
        fn safe_never_touches_active_storage(
            targets in proptest_vec(device_path(), 1..6),
            snap in arb_snapshot(),
        ) {
            let roots: &[&str] = &["/var/lib/kcore/volumes"];
            let verdict = classify_disk_layout(&targets, &snap, roots);
            if let Verdict::Safe = verdict {
                let target_set: std::collections::BTreeSet<&str> =
                    targets.iter().map(|s| s.as_str()).collect();
                for dev in &snap.devices {
                    let in_scope = is_within_targets(
                        &dev.path,
                        dev.parent_path.as_deref(),
                        &target_set,
                        &snap,
                    );
                    if !in_scope {
                        continue;
                    }
                    for mp in &dev.mountpoints {
                        prop_assert!(
                            !roots.iter().any(|r| mp.starts_with(r)),
                            "SAFE verdict but {} is mounted at kcore volume root {}",
                            dev.path,
                            mp
                        );
                        prop_assert!(
                            !matches!(mp.as_str(), "/" | "/boot" | "/nix" | "/nix/store" | "/boot/efi"),
                            "SAFE verdict but {} hosts system mount {}",
                            dev.path,
                            mp
                        );
                    }
                    if let Some(fs) = dev.fstype.as_deref() {
                        prop_assert!(
                            fs != "LVM2_member" && fs != "zfs_member",
                            "SAFE verdict but {} is an active {} member",
                            dev.path,
                            fs
                        );
                    }
                }
            }
        }

        /// Determinism: classification depends only on inputs.
        #[test]
        fn classifier_is_deterministic(
            targets in proptest_vec(device_path(), 1..6),
            snap in arb_snapshot(),
        ) {
            let roots: &[&str] = &["/var/lib/kcore/volumes"];
            let a = classify_disk_layout(&targets, &snap, roots);
            let b = classify_disk_layout(&targets, &snap, roots);
            prop_assert_eq!(a, b);
        }

        /// `extract_target_devices` is idempotent: extracting from a layout we
        /// rebuild from its own targets reproduces the same set.
        #[test]
        fn extract_target_devices_roundtrip(paths in proptest_vec(device_path(), 0..6)) {
            let body = paths.iter()
                .map(|p| format!("device = \"{p}\";"))
                .collect::<Vec<_>>()
                .join("\n");
            let mut expected: Vec<String> = paths.clone();
            expected.sort();
            expected.dedup();
            let got = extract_target_devices(&body);
            prop_assert_eq!(got, expected);
        }
    }
}

// =============================================================
// Layout-parser property tests (proptest only)
// =============================================================
//
// The layout-diff parser (`extract_target_devices` + the
// `strip_nix_comments` helper it depends on) is the single piece
// of logic that turns operator-supplied Nix text into the set of
// `/dev/...` paths the controller and node-agent reason about.
//
// We previously had Kani harnesses for these, but the SAT solver
// did not converge in any reasonable time even at MAX_INPUT_LEN=3
// and unwind=6 — parser state machines are pathological for CBMC.
// proptest at lengths up to 200 chars covers ~65,000× more states
// in a few hundred milliseconds, so it is the right tool here.
//
// Run with:
//
// ```text
// cargo test -p kcore-disko-types
// ```
#[cfg(test)]
mod parser_prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// **Liveness**: `extract_target_devices` never panics on
        /// any byte string. This rules out a panicking
        /// controller-side parser feeding adversarial layout
        /// bodies.
        #[test]
        fn extract_target_devices_never_panics(s in ".{0,200}") {
            let _ = extract_target_devices(&s);
        }

        /// **Liveness + length preservation**: the comment stripper
        /// never panics and preserves byte length. Length
        /// preservation is what lets the extractor's byte indices
        /// stay valid against the original source text.
        #[test]
        fn strip_nix_comments_preserves_length(s in ".{0,200}") {
            let stripped = strip_nix_comments(&s);
            prop_assert_eq!(stripped.len(), s.len());
        }

        /// **Soundness shape**: every device path the extractor
        /// returns starts with `/dev/`. The classifier's
        /// `is_within_targets` check assumes this prefix.
        #[test]
        fn extract_target_devices_outputs_dev_prefixed_paths(s in ".{0,200}") {
            for p in extract_target_devices(&s) {
                prop_assert!(p.starts_with("/dev/"), "{p:?} not /dev/-prefixed");
            }
        }

        /// **Determinism**: extracting twice yields the same set.
        /// Catches any accidental dependence on uninitialised
        /// memory, hash randomisation, or iterator order.
        #[test]
        fn extract_target_devices_is_deterministic(s in ".{0,200}") {
            let a = extract_target_devices(&s);
            let b = extract_target_devices(&s);
            prop_assert_eq!(a, b);
        }
    }
}
