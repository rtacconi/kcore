//! Axum router construction (shared by `main` and integration tests).

use axum::http::header;
use axum::routing::get;
use axum::Router;
use leptos::prelude::LeptosOptions;
use leptos_axum::{generate_route_list, LeptosRoutes};
use tower_http::trace::TraceLayer;

use crate::app::{shell, App};

/// Full dashboard HTTP stack (CSS, Leptos routes, server functions). Axum state is applied here, so
/// the result is `Router<()>` and can be used with [`axum::serve`].
pub fn dashboard_router(leptos_options: LeptosOptions) -> Router<()> {
    let routes = generate_route_list(App);
    Router::new()
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
        .with_state(leptos_options)
}
