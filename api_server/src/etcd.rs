use async_trait::async_trait;
use deadpool::managed;
use etcd_client::{Client, GetOptions, GetResponse, KeyValue};
use serde::{Deserialize, Serialize};

pub type EtcdPool = managed::Pool<EtcdManager>;
pub type EtcdClient = managed::Object<EtcdManager>;

#[derive(Debug, Deserialize)]
pub struct EtcdConfig {
    url: String,
}

pub struct EtcdManager {
    url: String,
}

pub struct EtcdError {
    pub msg: String,
    pub cause: Option<String>,
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

    async fn create(&self) -> core::result::Result<Self::Type, Self::Error> {
        let client = Client::connect([self.url.to_owned()], None).await?;
        Ok(client)
    }

    async fn recycle(&self, _: &mut Self::Type) -> managed::RecycleResult<Self::Error> {
        Ok(())
    }
}

impl EtcdConfig {
    pub async fn create_pool(&self) -> anyhow::Result<EtcdPool> {
        let manager = EtcdManager::from_config(self);
        let pool = managed::Pool::builder(manager).build()?;
        Ok(pool)
    }
}

impl EtcdError {
    pub fn new(msg: String, cause: Option<String>) -> Self {
        Self {
            msg,
            cause,
        }
    }
}

// all etcd error should be encapsulated into EtcdError
type Result<T> = core::result::Result<T, EtcdError>;

pub async fn put(client: &mut EtcdClient, key: String, value: impl Serialize) -> Result<()> {
    let value = serde_json::to_string(&value)
        .map_err(|err| EtcdError::new("Failed to serialize".into(), Some(err.to_string())))?;

    let _ = client
        .put(key, value, None)
        .await
        .map_err(|err| EtcdError::new("Failed to put".into(), Some(err.to_string())))?;
    tracing::debug!("Successfully put");
    Ok(())
}

pub async fn get(
    client: &mut EtcdClient,
    key: String,
    option: Option<GetOptions>,
) -> Result<GetResponse> {
    let res = client
        .get(key, option)
        .await
        .map_err(|err| EtcdError::new("Failed to get".into(), Some(err.to_string())))?;
    tracing::debug!("Successfully get");
    Ok(res)
}

pub fn kv_to_str(kv: &KeyValue) -> Result<(String, String)> {
    let (key_str, val_str) = (
        kv.key_str().map_err(|err| {
            EtcdError::new(
                "Failed to convert key to String".into(),
                Some(err.to_string()),
            )
        })?,
        kv.value_str().map_err(|err| {
            EtcdError::new(
                "Failed to convert value to String".into(),
                Some(err.to_string()),
            )
        })?,
    );
    Ok((key_str.to_string(), val_str.to_string()))
}
