use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use futures_util::future::BoxFuture;
use reflector::{Reflector, ReflectorNotification};
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

mod reflector;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub type Store = DashMap<String, String>;

pub type CLS<ARG, RES> = Box<dyn Fn(ARG) -> BoxFuture<'static, Result<RES>> + Send + Sync>;

pub struct ListerWatcher {
    pub lister: CLS<(), Vec<(String, String)>>,
    pub watcher: CLS<(), WsStream>,
}

pub struct EventHandler {
    pub add_cls: CLS<String, ()>,
    pub update_cls: CLS<(String, String), ()>,
    pub delete_cls: CLS<String, ()>,
}

pub struct Informer {
    reflector: Arc<Reflector>,
    eh: EventHandler,
}

impl Informer {
    pub fn new(lw: ListerWatcher, eh: EventHandler) -> Self {
        let reflector = Reflector {
            lw,
            store: DashMap::new(),
        };
        Self {
            reflector: Arc::new(reflector),
            eh,
        }
    }

    pub async fn run(&self) -> Result<()> {
        // start reflector
        let (tx, mut rx) = mpsc::channel::<ReflectorNotification>(16);
        let r = self.reflector.clone();
        let reflector_handle = tokio::spawn(async move { r.run(tx).await });

        while let Some(n) = rx.recv().await {
            match n {
                ReflectorNotification::Add(new) => {
                    (self.eh.add_cls)(new).await?;
                },
                ReflectorNotification::Update(old, new) => {
                    (self.eh.update_cls)((old, new)).await?;
                },
                ReflectorNotification::Delete(old) => {
                    (self.eh.delete_cls)(old).await?;
                },
            }
        }

        reflector_handle.await?
    }
}
