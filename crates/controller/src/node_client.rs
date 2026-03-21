use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tonic::transport::Channel;
use tracing::info;

use crate::node_proto;

type ComputeClient = node_proto::node_compute_client::NodeComputeClient<Channel>;
type AdminClient = node_proto::node_admin_client::NodeAdminClient<Channel>;

#[derive(Clone)]
pub struct NodeClients {
    clients: Arc<Mutex<HashMap<String, (ComputeClient, AdminClient)>>>,
}

impl NodeClients {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect(
        &self,
        address: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = format!("http://{address}");
        let channel = Channel::from_shared(endpoint.clone())?.connect().await?;
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
