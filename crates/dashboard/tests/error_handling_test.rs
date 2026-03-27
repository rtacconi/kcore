//! Tests that the dashboard renders graceful error messages when the controller is unreachable.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use kcore_dashboard::app_server::dashboard_router;
use kcore_dashboard::config::DashboardConfig;
use kcore_dashboard::state::set_dashboard_config;
use leptos::config::get_configuration;
use tower::ServiceExt;

async fn fetch(app: &axum::Router, path: &str) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 8 * 1024 * 1024).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn pages_render_error_messages_when_controller_is_down() {
    // Point at an address where nothing is listening.
    set_dashboard_config(DashboardConfig::insecure_on("127.0.0.1:1")).expect("set config");

    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let conf = get_configuration(Some(manifest)).expect("leptos config");
    let app = dashboard_router(conf.leptos_options);

    // Homepage (static) should still render fine — no gRPC needed.
    let (st, home) = fetch(&app, "/").await;
    assert_eq!(st, StatusCode::OK);
    assert!(
        home.contains("Declarative virtualization"),
        "homepage is static and must render without controller"
    );

    // CSS should still serve.
    let (st, css) = fetch(&app, "/dashboard.css").await;
    assert_eq!(st, StatusCode::OK);
    assert!(css.contains("kcore dashboard"));

    // Data pages should return 200 with inline error messages (Leptos SSR renders the error).
    let (st, compliance) = fetch(&app, "/compliance").await;
    assert_eq!(st, StatusCode::OK, "compliance page HTTP status");
    assert!(
        compliance.contains("error running server function")
            || compliance.contains("ServerError")
            || compliance.contains("transport error")
            || compliance.contains("gRPC"),
        "compliance must show connection error; excerpt: {:?}",
        compliance.chars().take(600).collect::<String>()
    );

    let (st, vms) = fetch(&app, "/vms").await;
    assert_eq!(st, StatusCode::OK, "VMs page HTTP status");
    assert!(
        vms.contains("error running server function")
            || vms.contains("ServerError")
            || vms.contains("transport error"),
        "VMs page must show connection error"
    );

    let (st, nets) = fetch(&app, "/networks").await;
    assert_eq!(st, StatusCode::OK, "networks page HTTP status");
    assert!(
        nets.contains("error running server function")
            || nets.contains("ServerError")
            || nets.contains("transport error"),
        "networks page must show connection error"
    );
}
