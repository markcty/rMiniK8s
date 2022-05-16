use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use futures_util::future::BoxFuture;
use reflector::{Reflector, ReflectorNotification};
use tokio::{
    net::TcpStream,
    select,
    sync::{mpsc, RwLock},
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::{informer::reflector::ResyncNotification, objects::Object};

mod reflector;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub type Store<T> = Arc<RwLock<HashMap<String, T>>>;

pub type CLS<ARG, RES> = Box<dyn Fn(ARG) -> BoxFuture<'static, Result<RES>> + Send + Sync>;

pub struct ListerWatcher<T: Object> {
    pub lister: CLS<(), Vec<T>>,
    pub watcher: CLS<(), WsStream>,
}

pub struct EventHandler<T: Object> {
    pub add_cls: CLS<T, ()>,
    pub update_cls: CLS<(T, T), ()>,
    pub delete_cls: CLS<T, ()>,
}

pub struct ResyncHandler(pub CLS<(), ()>);

pub struct Informer<T: Object> {
    reflector: Arc<Reflector<T>>,
    eh: EventHandler<T>,
    rh: ResyncHandler,
}

impl<T: Object> Informer<T> {
    pub fn new(lw: ListerWatcher<T>, eh: EventHandler<T>, rh: ResyncHandler) -> Self {
        let store = Arc::new(RwLock::new(HashMap::new()));
        let reflector = Reflector {
            lw,
            store,
        };
        Self {
            reflector: Arc::new(reflector),
            eh,
            rh,
        }
    }

    pub fn get_store(&self) -> Arc<RwLock<HashMap<String, T>>> {
        self.reflector.store.clone()
    }

    pub async fn run(&self) -> Result<()> {
        // start reflector
        let (tx, mut rx) = mpsc::channel::<ReflectorNotification<T>>(16);
        let (resync_tx, mut resync_rx) = mpsc::channel::<ResyncNotification>(4);
        let r = self.reflector.clone();
        let mut reflector_handle = tokio::spawn(async move { r.run(tx, resync_tx).await });

        tracing::info!("Informer started");

        loop {
            select! {
                Some(n) = rx.recv() => {
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
                },
                Some(_) = resync_rx.recv() => {
                    (self.rh.0)(()).await?;
                },
                reflector_result = &mut reflector_handle => {
                    if let Err(e) = reflector_result {
                        tracing::error!("Reflector ended unexpectedly, caused by: {}", e);
                    }
                }
            };
        }
    }
}
