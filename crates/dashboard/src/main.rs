//! HTTP server for the kcore dashboard.

use anyhow::Context;
use kcore_dashboard::app_server::dashboard_router;
use kcore_dashboard::config::DashboardConfig;
use kcore_dashboard::state::set_dashboard_config;
use leptos::config::get_configuration;
use leptos::logging;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls CryptoProvider");

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = DashboardConfig::from_env().context(
        "controller connection: set KCORE_CONTROLLER (or CONTROLLER_ADDR), \
         KCORE_CA_FILE, KCORE_CERT_FILE, KCORE_KEY_FILE, or KCORE_INSECURE=1 for plaintext",
    )?;
    set_dashboard_config(cfg)
        .map_err(|_| anyhow::anyhow!("dashboard config already initialized"))?;

    let conf =
        get_configuration(None).context("Leptos configuration (see package.metadata.leptos)")?;
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let app = dashboard_router(leptos_options);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    logging::log!("kcore-dashboard listening on http://{}", addr);
    axum::serve(listener, app).await.context("axum serve")?;
    Ok(())
}
