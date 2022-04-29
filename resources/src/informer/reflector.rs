use anyhow::{anyhow, Result};
use futures_util::stream::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::{ListerWatcher, Store};
use crate::models::etcd::WatchEvent;

pub(super) struct Reflector {
    pub(super) lw: ListerWatcher,
    pub(super) store: Store,
}

#[derive(Debug)]
pub(super) enum ReflectorNotification {
    Add(String),
    /// old value, new value
    Update(String, String),
    Delete(String),
}

impl Reflector {
    pub(super) async fn run(&self, tx: mpsc::Sender<ReflectorNotification>) -> Result<()> {
        // pull the init changes
        let kvs: Vec<(String, String)> = (self.lw.lister)(()).await?;
        for (k, v) in kvs {
            self.store.insert(k, v);
        }
        let (_, mut receiver) = (self.lw.watcher)(()).await?.split();

        loop {
            let msg: Message = receiver
                .next()
                .await
                .ok_or_else(|| anyhow!("Failed to receive watch message from api-server"))??;

            if msg.is_close() {
                return Err(anyhow!("Api-server watch disconnect"));
            }

            if let Message::Text(msg) = msg {
                let event: WatchEvent = serde_json::from_str(msg.as_str())?;
                match event {
                    WatchEvent::Put(e) => {
                        if let Some(object) = self.store.get(&e.key) {
                            let old = object.clone();
                            drop(object);

                            self.store.insert(e.key.to_owned(), e.object.clone());
                            tx.send(ReflectorNotification::Update(old, e.object))
                                .await?;
                        } else {
                            self.store.insert(e.key.to_owned(), e.object.clone());
                            tx.send(ReflectorNotification::Add(e.object)).await?;
                        }
                    },
                    WatchEvent::Delete(e) => {
                        if let Some(old) = self.store.remove(&e.key) {
                            tx.send(ReflectorNotification::Delete(old.1)).await?;
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
