use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::info;

use crate::node_proto;

type ComputeClient = node_proto::node_compute_client::NodeComputeClient<Channel>;
type AdminClient = node_proto::node_admin_client::NodeAdminClient<Channel>;

#[derive(Clone)]
pub struct TlsClientConfig {
    pub ca_file: String,
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Clone)]
pub struct NodeClients {
    clients: Arc<Mutex<HashMap<String, (ComputeClient, AdminClient)>>>,
    tls: Option<TlsClientConfig>,
}

impl NodeClients {
    pub fn new(tls: Option<TlsClientConfig>) -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            tls,
        }
    }

    pub async fn connect(
        &self,
        address: &str,
    ) -> Result<()> {
        let channel = if let Some(tls_cfg) = &self.tls {
            let endpoint = format!("https://{address}");
            let ca_pem = std::fs::read_to_string(&tls_cfg.ca_file)?;
            let cert_pem = std::fs::read_to_string(&tls_cfg.cert_file)?;
            let key_pem = std::fs::read_to_string(&tls_cfg.key_file)?;
            let tls = ClientTlsConfig::new()
                .ca_certificate(Certificate::from_pem(ca_pem))
                .identity(Identity::from_pem(cert_pem, key_pem));
            Channel::from_shared(endpoint)?
                .tls_config(tls)?
                .connect()
                .await?
        } else {
            let endpoint = format!("http://{address}");
            Channel::from_shared(endpoint)?.connect().await?
        };
        let compute = ComputeClient::new(channel.clone());
        let admin = AdminClient::new(channel);

        info!(address, "connected to node");
        self.clients
            .lock()
            .unwrap()
            .insert(address.to_string(), (compute, admin));
        Ok(())
    }

    pub fn get_compute(&self, address: &str) -> Option<ComputeClient> {
        self.clients
            .lock()
            .unwrap()
            .get(address)
            .map(|(c, _)| c.clone())
    }

    pub fn get_admin(&self, address: &str) -> Option<AdminClient> {
        self.clients
            .lock()
            .unwrap()
            .get(address)
            .map(|(_, a)| a.clone())
    }
}
