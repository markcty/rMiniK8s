use anyhow::Result;
use async_trait::async_trait;
use deadpool::managed;
use etcd_client::Client;
use serde::Deserialize;

pub type EtcdPool = managed::Pool<EtcdManager>;
pub type EtcdClient = managed::Object<EtcdManager>;

#[derive(Debug, Deserialize)]
pub struct EtcdConfig {
    url: String,
}

pub struct EtcdManager {
    url: String,
}

impl EtcdManager {
    fn from_config(config: &EtcdConfig) -> EtcdManager {
        EtcdManager {
            url: config.url.to_owned(),
        }
    }
}

#[async_trait]
impl managed::Manager for EtcdManager {
    type Type = etcd_client::Client;
    type Error = etcd_client::Error;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        let client = Client::connect([self.url.to_owned()], None).await?;
        Ok(client)
    }

    async fn recycle(&self, _: &mut Self::Type) -> managed::RecycleResult<Self::Error> {
        Ok(())
    }
}

impl EtcdConfig {
    pub async fn create_pool(&self) -> Result<EtcdPool> {
        let manager = EtcdManager::from_config(&self);
        let pool = managed::Pool::builder(manager).build()?;
        Ok(pool)
    }
}
