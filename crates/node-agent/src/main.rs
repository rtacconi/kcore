mod config;
mod discovery;
mod grpc;
mod vmm;

use clap::Parser;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tracing::info;

pub mod proto {
    tonic::include_proto!("kcore.node");
}

#[derive(Parser)]
#[command(name = "kcore-node-agent", about = "kcore node agent")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "/etc/kcore/node-agent.yaml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = config::Config::load(&cli.config)?;

    let addr = cfg.listen_addr.parse()?;
    let vm_client = vmm::Client::new(&cfg.vm_socket_dir);

    let compute_svc = proto::node_compute_server::NodeComputeServer::new(
        grpc::ComputeService::new(vm_client.clone()),
    );
    let info_svc =
        proto::node_info_server::NodeInfoServer::new(grpc::InfoService::new(cfg.node_id.clone()));
    let admin_svc = proto::node_admin_server::NodeAdminServer::new(grpc::AdminService::new(
        cfg.nix_config_path.clone(),
    ));
    let storage_svc =
        proto::node_storage_server::NodeStorageServer::new(grpc::StorageService::new());

    info!(addr = %addr, node_id = %cfg.node_id, "starting node-agent");

    let mut server = Server::builder();
    if let Some(tls) = cfg.tls.as_ref() {
        let cert_pem = std::fs::read_to_string(&tls.cert_file)?;
        let key_pem = std::fs::read_to_string(&tls.key_file)?;
        let ca_pem = std::fs::read_to_string(&tls.ca_file)?;
        let server_tls = ServerTlsConfig::new()
            .identity(Identity::from_pem(cert_pem, key_pem))
            .client_ca_root(Certificate::from_pem(ca_pem));
        server = server.tls_config(server_tls)?;
    }

    server
        .add_service(compute_svc)
        .add_service(info_svc)
        .add_service(admin_svc)
        .add_service(storage_svc)
        .serve(addr)
        .await?;

    Ok(())
}
