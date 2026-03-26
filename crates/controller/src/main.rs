mod auth;
mod config;
mod db;
mod grpc;
mod nixgen;
mod node_client;
mod scheduler;

use clap::Parser;
use tokio::signal;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tracing::{info, warn};

pub mod controller_proto {
    tonic::include_proto!("kcore.controller");
}

pub mod node_proto {
    tonic::include_proto!("kcore.node");
}

#[derive(Parser)]
#[command(name = "kcore-controller", about = "kcore controller")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "/etc/kcore/controller.yaml")]
    config: String,

    /// Allow running without TLS (INSECURE: all RPCs are unauthenticated)
    #[arg(long)]
    allow_insecure: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = config::Config::load(&cli.config)?;
    let addr = cfg.listen_addr.parse()?;

    if cfg.tls.is_none() && !cli.allow_insecure {
        anyhow::bail!(
            "TLS is not configured. All gRPC traffic would be unauthenticated and unencrypted.\n\
             Configure a [tls] section in the config file, or pass --allow-insecure to override."
        );
    }

    let database = db::Database::open(&cfg.db_path)?;
    let clients =
        node_client::NodeClients::new(cfg.tls.as_ref().map(|tls| node_client::TlsClientConfig {
            ca_file: tls.ca_file.clone(),
            cert_file: tls.cert_file.clone(),
            key_file: tls.key_file.clone(),
        }));

    let controller_svc =
        controller_proto::controller_server::ControllerServer::new(grpc::ControllerService::new(
            database.clone(),
            clients.clone(),
            cfg.default_network.clone(),
        ));

    let admin_svc = controller_proto::controller_admin_server::ControllerAdminServer::new(
        grpc::ControllerAdminService::new(),
    );

    let (mut health_reporter, health_svc) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<controller_proto::controller_server::ControllerServer<grpc::ControllerService>>()
        .await;

    let mut server = Server::builder();
    if let Some(tls) = cfg.tls.as_ref() {
        let cert_pem = std::fs::read_to_string(&tls.cert_file)?;
        let key_pem = std::fs::read_to_string(&tls.key_file)?;
        let ca_pem = std::fs::read_to_string(&tls.ca_file)?;
        let server_tls = ServerTlsConfig::new()
            .identity(Identity::from_pem(cert_pem, key_pem))
            .client_ca_root(Certificate::from_pem(ca_pem));
        server = server.tls_config(server_tls)?;
        info!(addr = %addr, "starting controller with mTLS");
    } else {
        warn!(addr = %addr, "starting controller WITHOUT TLS (--allow-insecure) — all RPCs are unauthenticated");
    }

    let staleness_db = database.clone();
    tokio::spawn(async move {
        const HEARTBEAT_TIMEOUT_SECS: i64 = 90;
        const CHECK_INTERVAL_SECS: u64 = 30;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(CHECK_INTERVAL_SECS)).await;
            match staleness_db.get_stale_nodes(HEARTBEAT_TIMEOUT_SECS) {
                Ok(stale) => {
                    for node in &stale {
                        if staleness_db
                            .update_node_status(&node.id, "not-ready")
                            .unwrap_or(false)
                        {
                            warn!(
                                node_id = %node.id,
                                last_heartbeat = %node.last_heartbeat,
                                "node missed heartbeat deadline, marked not-ready"
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "failed to check for stale nodes");
                }
            }
        }
    });

    server
        .add_service(health_svc)
        .add_service(controller_svc)
        .add_service(admin_svc)
        .serve_with_shutdown(addr, shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = signal::ctrl_c();
    #[cfg(unix)]
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
        .expect("failed to register SIGTERM handler");
    #[cfg(unix)]
    let terminate = sigterm.recv();
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { info!("received Ctrl+C, shutting down"); },
        _ = terminate => { info!("received SIGTERM, shutting down"); },
    }
}
