//! App state, page enum, and keyboard / exit policy (dev vs production).

use crate::inventory::Snapshot;

/// Top-level TUI pages (tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Overview,
    Network,
    Storage,
    Diagnostics,
    Help,
}

impl Page {
    pub const ALL: [Page; 5] = [
        Page::Overview,
        Page::Network,
        Page::Storage,
        Page::Diagnostics,
        Page::Help,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Network => "Network",
            Page::Storage => "Storage",
            Page::Diagnostics => "Diagnostics",
            Page::Help => "Help",
        }
    }

    pub fn next(self) -> Page {
        match self {
            Page::Overview => Page::Network,
            Page::Network => Page::Storage,
            Page::Storage => Page::Diagnostics,
            Page::Diagnostics => Page::Help,
            Page::Help => Page::Overview,
        }
    }

    pub fn prev(self) -> Page {
        match self {
            Page::Overview => Page::Help,
            Page::Network => Page::Overview,
            Page::Storage => Page::Network,
            Page::Diagnostics => Page::Storage,
            Page::Help => Page::Diagnostics,
        }
    }
}

/// Row selection for scrollable tables (per page).
#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub page: Page,
    pub dev: bool,
    /// Selection index for Network and Storage table rows.
    pub network_sel: usize,
    pub storage_sel: usize,
    pub last_msg: Option<String>,
    pub snapshot: Snapshot,
}

impl AppState {
    pub fn new(dev: bool, initial: Snapshot) -> Self {
        Self {
            page: Page::default(),
            dev,
            network_sel: 0,
            storage_sel: 0,
            last_msg: None,
            snapshot: initial,
        }
    }

    /// When `true`, the `q` key exits the TUI. Only enabled in dev mode.
    pub fn allow_exit_on_q(&self) -> bool {
        self.dev
    }

    pub fn clamp_network_selection(&mut self) {
        let n = self.snapshot.nics.len();
        if n == 0 {
            self.network_sel = 0;
        } else {
            self.network_sel = self.network_sel.min(n - 1);
        }
    }

    pub fn clamp_storage_selection(&mut self) {
        let n = self.snapshot.disks.len();
        if n == 0 {
            self.storage_sel = 0;
        } else {
            self.storage_sel = self.storage_sel.min(n - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, Page, Snapshot};

    #[test]
    fn page_tabs_wrap() {
        assert_eq!(Page::Overview.next(), Page::Network);
        assert_eq!(Page::Help.next(), Page::Overview);
        assert_eq!(Page::Overview.prev(), Page::Help);
    }

    #[test]
    fn production_mode_disallows_q_exit() {
        let s = AppState::new(false, Snapshot::default());
        assert!(!s.allow_exit_on_q());
    }

    #[test]
    fn dev_mode_allows_q_exit() {
        let s = AppState::new(true, Snapshot::default());
        assert!(s.allow_exit_on_q());
    }
}
