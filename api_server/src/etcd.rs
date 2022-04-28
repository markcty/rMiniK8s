use anyhow::anyhow;
use async_trait::async_trait;
use axum::extract::ws::{Message, WebSocket};
use deadpool::managed;
use etcd_client::*;
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use resources::models::etcd::WatchEvent;
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

pub async fn put(
    client: &mut EtcdClient,
    key: String,
    value: impl Serialize,
    option: Option<PutOptions>,
) -> Result<()> {
    let value = serde_json::to_string(&value)
        .map_err(|err| EtcdError::new("Failed to serialize".into(), Some(err.to_string())))?;

    let _ = client
        .put(key, value, option)
        .await
        .map_err(|err| EtcdError::new("Failed to put".into(), Some(err.to_string())))?;
    tracing::debug!("Successfully put");
    Ok(())
}

pub async fn forward_watch_to_ws(socket: WebSocket, watcher: Watcher, stream: WatchStream) {
    let watch_id = watcher.watch_id();
    let (sender, receiver) = socket.split();

    async fn ws_send(
        mut sender: SplitSink<WebSocket, Message>,
        watcher: Watcher,
        mut stream: WatchStream,
    ) -> anyhow::Result<()> {
        while let Some(res) = stream.message().await? {
            if res.canceled() {
                tracing::info!(
                    "Disconnect watch {}, caused by: {}",
                    watcher.watch_id(),
                    res.cancel_reason()
                );
                return Ok(());
            }

            for event in res.events() {
                let kv = event.kv().ok_or_else(|| anyhow!("Failed to get etcd kv"))?;
                let msg = match event.event_type() {
                    EventType::Delete => {
                        let key = kv.key_str()?.to_string();
                        let event = WatchEvent::new_delete(key);
                        Message::Text(serde_json::to_string(&event)?)
                    },
                    EventType::Put => {
                        if let Some(kv) = event.kv() {
                            let key = kv.key_str()?.to_string();
                            let value = kv.value_str()?.to_string();

                            let event = WatchEvent::new_put(key, serde_json::from_str(&value)?);
                            Message::Text(serde_json::to_string(&event)?)
                        } else {
                            continue;
                        }
                    },
                };
                sender.send(msg).await?;
            }
        }
        Ok(())
    }

    async fn ws_receive(mut receiver: SplitStream<WebSocket>) -> anyhow::Result<()> {
        while let Some(msg) = receiver.next().await {
            let msg = msg?;
            match msg {
                Message::Close(_) => return Ok(()),
                _ => continue,
            }
        }
        Ok(())
    }

    tokio::select! {
        res = ws_send(sender, watcher, stream) => {
            if let Err(e) = res {
                tracing::error!("Etcd watch {} exit unexpectedly, caused by: {}", watch_id, e.to_string());
            }
        },
        res = ws_receive(receiver) => {
            if let Err(e) = res {
                tracing::error!("Etcd watch {} exit unexpectedly, caused by: {}", watch_id, e.to_string());
            } else {
                tracing::info!("Etcd watch {} disconnected by client", watch_id);
            }
        },
    }
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

pub async fn delete(
    client: &mut EtcdClient,
    key: String,
    option: Option<DeleteOptions>,
) -> Result<()> {
    let _ = client
        .delete(key, option)
        .await
        .map_err(|err| EtcdError::new("Failed to delete".into(), Some(err.to_string())));
    tracing::debug!("Successfully delete");
    Ok(())
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
