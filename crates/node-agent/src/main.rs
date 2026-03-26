mod auth;
mod config;
mod discovery;
mod grpc;
mod registration;
mod storage;
mod vmm;

use clap::Parser;
use tokio::signal;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tracing::{info, warn};

pub mod proto {
    tonic::include_proto!("kcore.node");
}

pub mod controller_proto {
    tonic::include_proto!("kcore.controller");
}

#[derive(Parser)]
#[command(name = "kcore-node-agent", about = "kcore node agent")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "/etc/kcore/node-agent.yaml")]
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

    if cfg.tls.is_none() && !cli.allow_insecure {
        anyhow::bail!(
            "TLS is not configured. All gRPC traffic would be unauthenticated and unencrypted.\n\
             Configure a [tls] section in the config file, or pass --allow-insecure to override."
        );
    }

    let addr = cfg.listen_addr.parse()?;
    let vm_client = vmm::Client::new(&cfg.vm_socket_dir);
    let storage = storage::from_config(&cfg.storage).map_err(anyhow::Error::new)?;

    let compute_svc = proto::node_compute_server::NodeComputeServer::new(
        grpc::ComputeService::new(vm_client.clone()),
    );
    let info_svc =
        proto::node_info_server::NodeInfoServer::new(grpc::InfoService::new(cfg.node_id.clone()));
    let admin_svc =
        proto::node_admin_server::NodeAdminServer::new(grpc::AdminService::new_with_storage(
            cfg.nix_config_path.clone(),
            cfg.vm_socket_dir.clone(),
            storage.clone(),
        ))
        .max_decoding_message_size(1024 * 1024 * 1024)
        .max_encoding_message_size(64 * 1024 * 1024);
    let storage_svc = proto::node_storage_server::NodeStorageServer::new(
        grpc::StorageService::new_with_storage(storage),
    );

    let (mut health_reporter, health_svc) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<proto::node_compute_server::NodeComputeServer<grpc::ComputeService>>()
        .await;

    if !cfg.controller_addr.is_empty() {
        let reg_cfg = cfg.clone();
        tokio::spawn(async move {
            registration::register_with_controller(&reg_cfg).await;
        });
    }

    let mut server = Server::builder();
    if let Some(tls) = cfg.tls.as_ref() {
        let cert_pem = std::fs::read_to_string(&tls.cert_file)?;
        let key_pem = std::fs::read_to_string(&tls.key_file)?;
        let ca_pem = std::fs::read_to_string(&tls.ca_file)?;
        let server_tls = ServerTlsConfig::new()
            .identity(Identity::from_pem(cert_pem, key_pem))
            .client_ca_root(Certificate::from_pem(ca_pem));
        server = server.tls_config(server_tls)?;
        info!(addr = %addr, node_id = %cfg.node_id, "starting node-agent with mTLS");
    } else {
        warn!(addr = %addr, node_id = %cfg.node_id, "starting node-agent WITHOUT TLS (--allow-insecure) — all RPCs are unauthenticated");
    }

    server
        .add_service(health_svc)
        .add_service(compute_svc)
        .add_service(info_svc)
        .add_service(admin_svc)
        .add_service(storage_svc)
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
