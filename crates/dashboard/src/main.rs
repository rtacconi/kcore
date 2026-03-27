//! HTTP server for the kcore dashboard.

use anyhow::Context;
use axum::http::header;
use axum::routing::get;
use axum::Router;
use kcore_dashboard::app::{shell, App};
use kcore_dashboard::config::DashboardConfig;
use kcore_dashboard::state::set_dashboard_config;
use leptos::config::get_configuration;
use leptos::logging;
use leptos_axum::{generate_route_list, LeptosRoutes};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    let routes = generate_route_list(App);

    let app = Router::new()
        .route(
            "/dashboard.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../assets/dashboard.css"),
                )
            }),
        )
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(TraceLayer::new_for_http())
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    logging::log!("kcore-dashboard listening on http://{}", addr);
    axum::serve(listener, app.into_make_service())
        .await
        .context("axum serve")?;
    Ok(())
}
