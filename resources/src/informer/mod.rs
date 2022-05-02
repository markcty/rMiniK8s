use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use futures_util::future::BoxFuture;
use reflector::{Reflector, ReflectorNotification};
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::objects::Object;

mod reflector;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub type Store<T> = DashMap<String, T>;

pub type CLS<ARG, RES> = Box<dyn Fn(ARG) -> BoxFuture<'static, Result<RES>> + Send + Sync>;

pub struct ListerWatcher<T> {
    pub lister: CLS<(), Vec<T>>,
    pub watcher: CLS<(), WsStream>,
}

pub struct EventHandler<T> {
    pub add_cls: CLS<T, ()>,
    pub update_cls: CLS<(T, T), ()>,
    pub delete_cls: CLS<T, ()>,
}

pub struct Informer<T> {
    reflector: Arc<Reflector<T>>,
    eh: EventHandler<T>,
}

impl<T: Object> Informer<T> {
    pub fn new(lw: ListerWatcher<T>, eh: EventHandler<T>) -> Self {
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
        let (tx, mut rx) = mpsc::channel::<ReflectorNotification<T>>(16);
        let r = self.reflector.clone();
        let reflector_handle = tokio::spawn(async move { r.run(tx).await });

        tracing::info!("Informer started");
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
