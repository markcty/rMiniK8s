use std::{collections::HashMap, future::Future, hash::Hash, sync::Arc};

use anyhow::{anyhow, Result};
use futures_util::{future::BoxFuture, stream::TakeUntil, StreamExt};
use reqwest::{self, Url};
use tokio::{
    join,
    net::TcpStream,
    sync::{mpsc, oneshot, oneshot::Sender, RwLock},
};
use tokio_tungstenite::{
    tungstenite::{Message, WebSocket},
    MaybeTlsStream, WebSocketStream,
};

use crate::{models::etcd::WatchEvent, objects::KubeObject};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

type Store = Arc<RwLock<HashMap<String, KubeObject>>>;

struct Informer {
    store: Store,
    reflector: Arc<Reflector>,
}

#[derive(Debug)]
struct ReflectorNotification {
    key: String,
    op: ReflectorNotificationOp,
}

#[derive(Debug)]
enum ReflectorNotificationOp {
    Add(KubeObject),
    /// old value, new value
    Update(KubeObject, KubeObject),
    Delete(KubeObject),
}

struct Reflector {
    lw: ListerWatcher,
    // tx: Sender<ReflectorNotification>,
    store: Store,
}

// struct ListerWatcher<L, LF, W, WF>
// where
//     L: Fn() -> LF,
//     LF: Future<Output = Result<Vec<KubeObject>>>,
//     W: Fn() -> WF,
//     WF: Future<Output = Result<WsStream>>,
// {
//     pub lister: L,
//     pub watcher: W,
// }

struct ListerWatcher {
    lister: Box<dyn Fn() -> BoxFuture<'static, Result<Vec<KubeObject>>> + Send + Sync>,
    watcher: Box<dyn Fn() -> BoxFuture<'static, Result<WsStream>> + Send + Sync>,
}

impl Informer {
    fn new(lw: ListerWatcher) -> Self {
        let store = Arc::new(RwLock::new(HashMap::new()));
        let reflector = Reflector {
            lw: lw,
            store: store.clone(),
        };
        Self {
            store,
            reflector: Arc::new(reflector),
        }
    }

    async fn run(&self) -> anyhow::Result<()> {
        // start reflector
        let (tx, mut rx) = mpsc::channel::<ReflectorNotification>(16);
        let r = self.reflector.clone();
        let reflector_handle = tokio::spawn(async move { r.run(tx).await });

        while let Some(n) = rx.recv().await {
            println!("{:?}", n);
        }

        reflector_handle.await?
    }
}

impl Reflector {
    async fn run(&self, tx: mpsc::Sender<ReflectorNotification>) -> Result<()> {
        // pull the init changes
        let objects: Vec<KubeObject> = (self.lw.lister)().await?;
        for object in objects {
            self.store
                .write()
                .await
                // replace it with gen uri
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
                    WatchEvent::Put(put) => {
                        if let Some(object) = self.store.read().await.get(&put.key) {
                            tx.send(ReflectorNotification {
                                key: put.key,
                                op: ReflectorNotificationOp::Update(object.clone(), put.object),
                            })
                            .await?;
                        } else {
                            tx.send(ReflectorNotification {
                                key: put.key,
                                op: ReflectorNotificationOp::Add(put.object),
                            })
                            .await?;
                        }
                    },
                    WatchEvent::Delete => {},
                }
            } else {
                tracing::warn!("Receive none text watch message from api-server");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use tokio_tungstenite::connect_async;

    use crate::controller::*;

    #[tokio::test]
    async fn run_informer() -> Result<()> {
        // TODO: maybe some closure can simplify the tedious closure definition
        let lw = ListerWatcher {
            lister: Box::new(|| {
                Box::pin(async {
                    let res = reqwest::get("http://localhost:8080/api/v1/pods")
                        .await?
                        .json::<Vec<KubeObject>>()
                        .await?;
                    Ok::<Vec<KubeObject>, anyhow::Error>(res)
                })
            }),
            watcher: Box::new(|| {
                Box::pin(async {
                    let url = Url::parse("ws://localhost:8080/api/v1/pods")?;
                    let (stream, _) = connect_async(url).await?;
                    Ok::<WsStream, Error>(stream)
                })
            }),
        };

        let informer = Informer::new(lw);

        informer.run().await?;

        Ok(())
    }
}
