use anyhow::Result;
use dashmap::DashMap;
use futures_util::future::BoxFuture;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::objects::KubeObject;

mod informer;
mod reflector;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

type Store = DashMap<String, KubeObject>;

pub struct ListerWatcher {
    pub lister: Box<dyn Fn() -> BoxFuture<'static, Result<Vec<KubeObject>>> + Send + Sync>,
    pub watcher: Box<dyn Fn() -> BoxFuture<'static, Result<WsStream>> + Send + Sync>,
}
