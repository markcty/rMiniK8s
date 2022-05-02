use std::fmt::Debug;

use anyhow::{anyhow, Result};
use futures_util::stream::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::{ListerWatcher, Store};
use crate::{models::etcd::WatchEvent, objects::Object};

pub(super) struct Reflector<T> {
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

impl<T: Object> Reflector<T> {
    pub(super) async fn run(&self, tx: mpsc::Sender<ReflectorNotification<T>>) -> Result<()> {
        // pull the init changes
        let objects: Vec<T> = (self.lw.lister)(()).await?;
        for object in objects {
            let key = object.uri();
            self.store.insert(key, object);
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
                let event: WatchEvent<T> = serde_json::from_str(msg.as_str())?;
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
