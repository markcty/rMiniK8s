use std::{collections::HashMap, fmt::Debug};

use anyhow::{anyhow, Result};
use futures::stream::SplitStream;
use futures_util::stream::StreamExt;
use tokio::{sync::mpsc, time::Duration};
use tokio_tungstenite::tungstenite::Message;

use super::{ListerWatcher, Store, WsStream};
use crate::{models::etcd::WatchEvent, objects::Object};

pub(super) struct Reflector<T: Object> {
    pub(super) lw: ListerWatcher<T>,
    pub(super) store: Store<T>,
}

#[derive(Debug)]
pub(super) enum ReflectorNotification<T> {
    Add(T),
    /// old value, new value
    Update(T, T),
    Delete(T),
}

#[derive(Debug)]
pub(super) struct ResyncNotification;

impl<T: Object> Reflector<T> {
    pub(super) async fn run(
        &self,
        tx: mpsc::Sender<ReflectorNotification<T>>,
        resync_tx: mpsc::Sender<ResyncNotification>,
    ) -> Result<()> {
        loop {
            let mut should_resync = false;
            // lister
            {
                let objects;
                loop {
                    let result = (self.lw.lister)(()).await;
                    if let Err(e) = result {
                        tracing::warn!("list failed, caused by: {}", e);
                    } else if let Ok(result) = result {
                        objects = result;
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                let mut new_store = HashMap::new();
                for object in objects {
                    new_store.insert(object.uri(), object);
                }
                let mut old_store = self.store.write().await;
                if !old_store.eq(&new_store) {
                    should_resync = true;
                    old_store.clone_from(&new_store);
                }
            }
            if should_resync {
                tracing::warn!("Inconsitent store, resync...");
                resync_tx.send(ResyncNotification).await?;
            }
            tracing::info!("List succeeded");

            // watcher
            match (self.lw.watcher)(()).await {
                Ok(stream) => {
                    let (_, receiver) = stream.split();
                    tracing::info!("Watcher connected, receiving events...");
                    if let Err(e) = self.handle_watcher(tx.clone(), receiver).await {
                        tracing::warn!("Watcher ended unexpectedly, caused by: {}", e);
                        tracing::warn!("Restarting reflector")
                    }
                },
                Err(e) => tracing::warn!("Watcher failed, caused by: {}", e),
            }
        }
    }

    pub async fn handle_watcher(
        &self,
        tx: mpsc::Sender<ReflectorNotification<T>>,
        mut receiver: SplitStream<WsStream>,
    ) -> Result<()> {
        loop {
            let msg: Message = receiver
                .next()
                .await
                .ok_or_else(|| anyhow!("Failed to receive watch message from api-server"))??;

            if msg.is_close() {
                return Err(anyhow!("Api-server watch disconnect"));
            }

            if let Message::Text(msg) = msg {
                let event: WatchEvent<T> = serde_json::from_str(msg.as_str())?;
                let mut store = self.store.write().await;
                match event {
                    WatchEvent::Put(e) => {
                        if let Some(object) = store.get(&e.key) {
                            let old = object.clone();

                            if old == e.object {
                                tracing::debug!("Object {} is already up to date", e.key);
                                continue;
                            }

                            store.insert(e.key.to_owned(), e.object.clone());
                            tx.send(ReflectorNotification::Update(old, e.object))
                                .await?;
                        } else {
                            store.insert(e.key.to_owned(), e.object.clone());
                            tx.send(ReflectorNotification::Add(e.object)).await?;
                        }
                    },
                    WatchEvent::Delete(e) => {
                        if let Some(old) = store.remove(&e.key) {
                            tx.send(ReflectorNotification::Delete(old)).await?;
                        } else {
                            tracing::warn!("Watch inconsistent, key {} already deleted", e.key);
                        }
                    },
                }
            } else {
                tracing::warn!("Receive none text watch message from api-server");
            }
        }
    }
}
