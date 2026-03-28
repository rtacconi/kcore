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
        (self.page as usize) * self.page_size < self.total
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
}
