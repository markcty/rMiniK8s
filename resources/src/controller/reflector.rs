use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures_util::stream::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::{ListerWatcher, Store};
use crate::{models::etcd::WatchEvent, objects::KubeObject};

pub(super) struct Reflector {
    pub(super) lw: ListerWatcher,
    // tx: Sender<ReflectorNotification>,
    pub(super) store: Arc<Store>,
}

#[derive(Debug)]
pub(super) struct ReflectorNotification {
    pub(super) key: String,
    pub(super) op: ReflectorNotificationOp,
}

#[derive(Debug)]
pub(super) enum ReflectorNotificationOp {
    Add(KubeObject),
    /// old value, new value
    Update(KubeObject, KubeObject),
    Delete(KubeObject),
}

impl Reflector {
    pub(super) async fn run(&self, tx: mpsc::Sender<ReflectorNotification>) -> Result<()> {
        // pull the init changes
        let objects: Vec<KubeObject> = (self.lw.lister)().await?;
        for object in objects {
            self.store
                // TODO: replace it with gen uri
                .insert(format!("/api/v1/pods/{}", object.name()), object.clone());
        }
        let (_, mut receiver) = (self.lw.watcher)().await?.split();

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
                            tx.send(ReflectorNotification {
                                key: e.key,
                                op: ReflectorNotificationOp::Update(old, e.object),
                            })
                            .await?;
                        } else {
                            self.store.insert(e.key.to_owned(), e.object.clone());
                            tx.send(ReflectorNotification {
                                key: e.key,
                                op: ReflectorNotificationOp::Add(e.object),
                            })
                            .await?;
                        }
                    },
                    WatchEvent::Delete(e) => {
                        if let Some(old) = self.store.remove(&e.key) {
                            tx.send(ReflectorNotification {
                                key: e.key,
                                op: ReflectorNotificationOp::Delete(old.1),
                            })
                            .await?;
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
