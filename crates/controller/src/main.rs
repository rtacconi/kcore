mod auth;
mod config;
mod db;
mod grpc;
mod nixgen;
mod node_client;
mod scheduler;

use clap::Parser;
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

    server
        .add_service(controller_svc)
        .add_service(admin_svc)
        .serve(addr)
        .await?;

    Ok(())
}
