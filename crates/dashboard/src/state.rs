//! Process-wide controller connection settings for server functions.

use std::sync::OnceLock;

use crate::config::DashboardConfig;

static CFG: OnceLock<DashboardConfig> = OnceLock::new();

pub fn set_dashboard_config(cfg: DashboardConfig) -> Result<(), DashboardConfig> {
    CFG.set(cfg)
}

pub fn dashboard_config() -> &'static DashboardConfig {
    CFG.get().expect(
        "kcore-dashboard: configuration not initialized (call set_dashboard_config from main)",
    )
}
