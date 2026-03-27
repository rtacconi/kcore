//! kcore web dashboard (Leptos SSR, controller gRPC).

pub mod api;
pub mod app;
pub mod app_server;
pub mod config;
pub mod controller_client;
pub mod dto;
pub mod format;
pub mod mappers;
pub mod state;

pub use app::shell;

// Ensure server function registrations are linked.
#[allow(unused_imports)]
use api::{get_compliance_dto, list_networks_dto, list_vms_page};
