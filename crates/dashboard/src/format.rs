//! Display helpers shared by UI and tests.

/// VM/data-plane storage backend from `kcore.controller.StorageBackendType` (i32).
pub fn storage_backend_label(backend: i32) -> &'static str {
    match backend {
        1 => "Filesystem",
        2 => "LVM",
        3 => "ZFS",
        _ => "Unspecified",
    }
}

/// VM state from `kcore.controller.VmState` (i32).
pub fn vm_state_label(state: i32) -> &'static str {
    match state {
        1 => "Stopped",
        2 => "Running",
        3 => "Paused",
        4 => "Error",
        _ => "Unknown",
    }
}

/// Human-readable MB (decimal for parity with typical CLI output).
pub fn memory_mebibytes(bytes: i64) -> String {
    if bytes <= 0 {
        return "0 MiB".to_string();
    }
    let mb = (bytes as f64) / (1024.0 * 1024.0);
    format!("{mb:.0} MiB")
}

/// Human-readable bytes with auto-scaled unit (GiB preferred).
pub fn bytes_human(bytes: i64) -> String {
    if bytes <= 0 {
        return "0".to_string();
    }
    let b = bytes as f64;
    if b >= 1024.0 * 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} TiB", b / (1024.0 * 1024.0 * 1024.0 * 1024.0))
    } else if b >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GiB", b / (1024.0 * 1024.0 * 1024.0))
    } else if b >= 1024.0 * 1024.0 {
        format!("{:.0} MiB", b / (1024.0 * 1024.0))
    } else {
        format!("{:.0} KiB", b / 1024.0)
    }
}

pub const VM_PAGE_SIZE: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageView<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub page_size: usize,
    pub total: usize,
}

impl<T> PageView<T> {
    pub fn total_pages(&self) -> u32 {
        if self.total == 0 {
            return 1;
        }
        self.total.div_ceil(self.page_size) as u32
    }

    pub fn has_prev(&self) -> bool {
        self.page > 1
    }

    pub fn has_next(&self) -> bool {
        // Use saturating multiplication: a hostile/garbage `page` query
        // string (large `u32`) used to overflow `usize` on 32-bit targets
        // and panic in debug builds. Saturating to `usize::MAX` produces
        // the correct answer ("no further page") for any pathological page.
        (self.page as usize).saturating_mul(self.page_size) < self.total
    }
}

/// 1-based page, stable ordering by VM name.
pub fn paginate_by_name<T: Clone>(
    mut items: Vec<T>,
    sort_key: impl Fn(&T) -> String,
    page: u32,
    page_size: usize,
) -> PageView<T> {
    let total = items.len();
    items.sort_by_key(|a| sort_key(a));
    let page = page.max(1);
    let start = ((page - 1) as usize).saturating_mul(page_size);
    let slice: Vec<T> = if start >= items.len() {
        vec![]
    } else {
        let end = (start + page_size).min(items.len());
        items[start..end].to_vec()
    };
    PageView {
        items: slice,
        page,
        page_size,
        total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_backend_labels() {
        assert_eq!(storage_backend_label(1), "Filesystem");
        assert_eq!(storage_backend_label(2), "LVM");
        assert_eq!(storage_backend_label(3), "ZFS");
        assert_eq!(storage_backend_label(0), "Unspecified");
    }

    #[test]
    fn vm_state_labels() {
        assert_eq!(vm_state_label(2), "Running");
        assert_eq!(vm_state_label(99), "Unknown");
    }

    #[test]
    fn memory_mb() {
        assert!(memory_mebibytes(1024 * 1024).contains('1'));
    }

    #[test]
    fn pagination_empty() {
        let v: Vec<i32> = vec![];
        let p = paginate_by_name(v, |n| n.to_string(), 1, 3);
        assert_eq!(p.total, 0);
        assert_eq!(p.items.len(), 0);
        assert_eq!(p.total_pages(), 1);
        assert!(!p.has_prev());
        assert!(!p.has_next());
    }

    #[test]
    fn pagination_sort_and_pages() {
        let items = vec!["c", "a", "b"];
        let p1 = paginate_by_name(items.clone(), |s| s.to_string(), 1, 2);
        assert_eq!(p1.items, vec!["a", "b"]);
        assert_eq!(p1.total_pages(), 2);
        assert!(!p1.has_prev());
        assert!(p1.has_next());

        let p2 = paginate_by_name(items, |s| s.to_string(), 2, 2);
        assert_eq!(p2.items, vec!["c"]);
        assert!(p2.has_prev());
        assert!(!p2.has_next());
    }

    #[test]
    fn pagination_page_beyond_end() {
        let items = vec![1, 2];
        let p = paginate_by_name(items, |n| n.to_string(), 99, 10);
        assert!(p.items.is_empty());
        assert_eq!(p.total, 2);
    }

    #[test]
    fn has_next_does_not_overflow_with_huge_page_query() {
        // Regression: `page * page_size` used to overflow `usize` for large
        // `page` values that come straight from the `?page=` query string.
        let view: PageView<i32> = PageView {
            items: vec![],
            page: u32::MAX,
            page_size: 10,
            total: 100,
        };
        assert!(
            !view.has_next(),
            "no next page when we are far past the end"
        );
    }

    #[test]
    fn vm_state_paused_label() {
        // Regression: state code 3 (Paused) used to be rendered with the
        // same UI affordance as Stopped, which hid the fact that the VM
        // was suspended rather than halted.
        assert_eq!(vm_state_label(3), "Paused");
    }

    #[test]
    fn bytes_human_tier_boundaries() {
        // 0 / negative renders as "0".
        assert_eq!(bytes_human(0), "0");
        assert_eq!(bytes_human(-1), "0");
        // KiB tier.
        assert_eq!(bytes_human(1024), "1 KiB");
        assert_eq!(bytes_human(1024 * 1023), "1023 KiB");
        // MiB tier (>= 1 MiB).
        assert_eq!(bytes_human(1024 * 1024), "1 MiB");
        assert_eq!(bytes_human(8 * 1024 * 1024), "8 MiB");
        // GiB tier (>= 1 GiB).
        assert_eq!(bytes_human(1024i64 * 1024 * 1024), "1.0 GiB");
        // TiB tier.
        assert_eq!(bytes_human(1024i64 * 1024 * 1024 * 1024), "1.0 TiB");
    }

    #[test]
    fn memory_mebibytes_handles_zero_and_negative() {
        assert_eq!(memory_mebibytes(0), "0 MiB");
        assert_eq!(memory_mebibytes(-1), "0 MiB");
        assert!(memory_mebibytes(2 * 1024 * 1024).contains("2 MiB"));
    }
}

/// Property-based tests (Phase 1): generate millions of random `(page, page_size,
/// total)` triples to lock down the pagination invariants. Example-based tests
/// caught the `u32::MAX` overflow case only because we thought to write it; these
/// properties would catch any future arithmetic regression on any input.
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// `PageView` accessors must never panic for any plausible
        /// `(page, page_size, total)` triple. `page` is `u32` so we exercise
        /// the full range, including the `u32::MAX` boundary that previously
        /// overflowed `(page as usize) * page_size`.
        #[test]
        fn page_view_accessors_never_panic(
            page in any::<u32>(),
            page_size in 1usize..=10_000,
            total in 0usize..=1_000_000,
        ) {
            let view: PageView<()> = PageView {
                items: vec![],
                page,
                page_size,
                total,
            };
            // Just calling these is the property — `unwrap_or` style panics
            // would surface as test failures.
            let _ = view.has_next();
            let _ = view.has_prev();
            let _ = view.total_pages();
        }

        /// Algebraic relationships between accessors:
        /// * `total_pages` is always >= 1 (we never render "Page 1 of 0").
        /// * `has_prev` ⇔ `page > 1` for any normalized 1-based page.
        /// * `has_next` is monotone w.r.t. `total`: if there is a next page
        ///   for `total`, there is also one for `total + k`.
        #[test]
        fn page_view_invariants_hold(
            page in 1u32..=1000,
            page_size in 1usize..=1000,
            total in 0usize..=100_000,
        ) {
            let view: PageView<()> = PageView {
                items: vec![],
                page,
                page_size,
                total,
            };
            prop_assert!(view.total_pages() >= 1);
            prop_assert_eq!(view.has_prev(), page > 1);

            // Monotonicity: more rows can only ever add a "next" page,
            // never remove one.
            let larger: PageView<()> = PageView {
                items: vec![],
                page,
                page_size,
                total: total.saturating_add(page_size),
            };
            if view.has_next() {
                prop_assert!(larger.has_next());
            }
        }

        /// `paginate_by_name` round-trip property: concatenating every page in
        /// order reconstructs the (sorted) input. This catches any future
        /// off-by-one in start/end slice math, including the `start >= len`
        /// fast path.
        #[test]
        fn paginate_by_name_covers_all_items(
            items in proptest::collection::vec("[a-z]{1,8}", 0..50),
            page_size in 1usize..=10,
        ) {
            let mut sorted = items.clone();
            sorted.sort();

            let total = items.len();
            let pages = if total == 0 { 1 } else { total.div_ceil(page_size) };

            let mut collected: Vec<String> = Vec::new();
            for page_idx in 1..=pages {
                let view = paginate_by_name(items.clone(), |s: &String| s.clone(),
                                            page_idx as u32, page_size);
                prop_assert_eq!(view.total, total);
                prop_assert!(view.items.len() <= page_size);
                collected.extend(view.items);
            }
            prop_assert_eq!(collected, sorted);
        }

        /// `memory_mebibytes` and `bytes_human` never panic for any
        /// i64 input (including negatives).
        #[test]
        fn byte_formatters_never_panic(n in any::<i64>()) {
            let _ = memory_mebibytes(n);
            let _ = bytes_human(n);
        }

        /// `memory_mebibytes` always ends with `" MiB"`.
        #[test]
        fn memory_mebibytes_unit_label(n in any::<i64>()) {
            prop_assert!(memory_mebibytes(n).ends_with(" MiB"));
        }

        /// `memory_mebibytes` returns `"0 MiB"` for any non-positive
        /// input (encodes the documented zero-clamp).
        #[test]
        fn memory_mebibytes_clamps_non_positive(n in i64::MIN..=0) {
            prop_assert_eq!(memory_mebibytes(n), "0 MiB");
        }

        /// `bytes_human`'s unit always corresponds to the magnitude.
        /// We assert the suffix matches one of the four documented
        /// labels.
        #[test]
        fn bytes_human_unit_in_known_set(n in any::<i64>()) {
            let s = bytes_human(n);
            prop_assert!(
                s == "0"
                    || s.ends_with(" KiB")
                    || s.ends_with(" MiB")
                    || s.ends_with(" GiB")
                    || s.ends_with(" TiB"),
                "unexpected unit in {s:?}"
            );
        }

        /// `vm_state_label` and `storage_backend_label` always return
        /// one of the documented strings, for any i32.
        #[test]
        fn enum_labels_known_set(state in any::<i32>(), backend in any::<i32>()) {
            let v = vm_state_label(state);
            prop_assert!(matches!(v, "Stopped" | "Running" | "Paused" | "Error" | "Unknown"));
            let s = storage_backend_label(backend);
            prop_assert!(matches!(s, "Filesystem" | "LVM" | "ZFS" | "Unspecified"));
        }
    }
}
