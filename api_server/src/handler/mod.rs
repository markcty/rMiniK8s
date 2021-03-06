use resources::models::ErrResponse;

use crate::{etcd::EtcdClient, AppState};

pub mod binding;
pub mod function;
pub mod gpu_job;
pub mod hpa;
pub mod ingress;
pub mod metrics;
pub mod node;
pub mod pod;
pub mod replica_set;
mod response;
pub mod service;
mod utils;
pub mod workflow;

impl AppState {
    pub async fn get_client(&self) -> Result<EtcdClient, ErrResponse> {
        let client = self.etcd_pool.get().await.map_err(|_| {
            tracing::error!("Failed to get etcd client");
            ErrResponse::new("Failed to get etcd Client".to_string(), None)
        })?;
        Ok(client)
    }
}
