use std::sync::Arc;

use actix::{prelude::*, Actor};
use anyhow::{Context, Result};
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

pub trait EventHandler<T>: Actor + Handler<Event<T>> {}

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub enum Event<T> {
    Add(T),
    Update(T, T),
    Delete(T),
}

pub struct Informer<T, H: EventHandler<T>> {
    reflector: Arc<Reflector<T>>,
    eh: Addr<H>,
}

impl<T: Object, H: EventHandler<T>> Informer<T, H> {
    pub fn new(lw: ListerWatcher<T>, eh: Addr<H>, store: Arc<Store<T>>) -> Self {
        let reflector = Reflector {
            lw,
            store,
        };
        Self {
            reflector: Arc::new(reflector),
            eh,
        }
    }

    pub async fn run(&self) -> Result<()>
    where
        <H as actix::Actor>::Context: actix::dev::ToEnvelope<H, Event<T>>,
    {
        // start reflector
        let (tx, mut rx) = mpsc::channel::<ReflectorNotification<T>>(16);
        let r = self.reflector.clone();
        let reflector_handle = tokio::spawn(async move { r.run(tx).await });

        tracing::info!("Informer started");
        while let Some(n) = rx.recv().await {
            let msg = match n {
                ReflectorNotification::Add(new) => Event::Add(new),
                ReflectorNotification::Update(old, new) => Event::Update(old, new),
                ReflectorNotification::Delete(old) => Event::Delete(old),
            };
            self.eh
                .send(msg)
                .await?
                .with_context(|| "EventHandler error")?;
        }

        reflector_handle.await?
    }
}
