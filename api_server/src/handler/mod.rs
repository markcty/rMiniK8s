use self::response::ErrResponse;
use crate::{etcd::EtcdClient, AppState};

pub mod node;
pub mod pod;
mod response;
mod utils;

impl AppState {
    pub async fn get_client(&self) -> Result<EtcdClient, ErrResponse> {
        let client = self.etcd_pool.get().await.map_err(|_| {
            tracing::error!("Failed to get etcd client");
            ErrResponse::new("Failed to get etcd Client".to_string(), None)
        })?;
        Ok(client)
    }
}
