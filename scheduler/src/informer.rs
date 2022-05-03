use std::sync::Arc;

use anyhow::{anyhow, Error};
use dashmap::DashMap;
use reqwest::Url;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, WsStream},
    models,
    objects::KubeObject,
};
use tokio::{
    sync::mpsc::{self, Receiver},
    task::JoinHandle,
};
use tokio_tungstenite::connect_async;

pub type RunInformerResult = (
    Receiver<KubeObject>,
    Arc<DashMap<String, KubeObject>>,
    JoinHandle<Result<(), Error>>,
);

pub fn run_pod_informer() -> RunInformerResult {
    // create list watcher closures
    // TODO: maybe some crate or macros can simplify the tedious boxed closure creation in heap
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get("http://localhost:8080/api/v1/pods")
                    .await?
                    .json::<models::Response<Vec<KubeObject>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<KubeObject>, Error>(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let url = Url::parse("ws://localhost:8080/api/v1/watch/pods")?;
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    };

    // create event handler closures
    let (tx_add, rx) = mpsc::channel::<KubeObject>(16);
    let eh = EventHandler::<KubeObject> {
        add_cls: Box::new(move |pod| {
            // TODO: this is not good: tx is copied every time add_cls is called, but I can't find a better way
            let tx_add = tx_add.clone();
            Box::pin(async move {
                if pod.kind() == "pod" {
                    tracing::debug!("add\n{}", pod.name());
                    tx_add.send(pod).await?;
                } else {
                    tracing::error!("There are some errors with the kind of object.");
                }
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old, new)| {
            Box::pin(async move {
                tracing::debug!("update\n{}\n{}", old.name(), new.name());
                Ok(())
            })
        }),
        delete_cls: Box::new(move |old| {
            Box::pin(async move {
                tracing::debug!("delete\n{}", old.name());
                Ok(())
            })
        }),
    };

    // start the informer
    let store = Arc::new(DashMap::new());
    let informer = Informer::new(lw, eh, store.clone());
    let informer_handler = tokio::spawn(async move { informer.run().await });

    (rx, store, informer_handler)
}

pub fn run_node_informer() -> RunInformerResult {
    // create list watcher closures
    // TODO: maybe some crate or macros can simplify the tedious boxed closure creation in heap
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get("http://localhost:8080/api/v1/nodes")
                    .await?
                    .json::<models::Response<Vec<KubeObject>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<KubeObject>, Error>(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let url = Url::parse("ws://localhost:8080/api/v1/watch/nodes")?;
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    };

    // create event handler closures
    let (_, rx) = mpsc::channel::<KubeObject>(16);
    let eh = EventHandler::<KubeObject> {
        add_cls: Box::new(move |node| {
            // TODO: this is not good: tx is copied every time add_cls is called, but I can't find a better way
            Box::pin(async move {
                tracing::debug!("add\n{}", node.name());
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old, new)| {
            Box::pin(async move {
                tracing::debug!("update\n{}\n{}", old.name(), new.name());
                Ok(())
            })
        }),
        delete_cls: Box::new(move |old| {
            Box::pin(async move {
                tracing::debug!("delete\n{}", old.name());
                Ok(())
            })
        }),
    };

    // start the informer
    let store = Arc::new(DashMap::new());
    let informer = Informer::new(lw, eh, store.clone());
    let informer_handler = tokio::spawn(async move { informer.run().await });

    (rx, store, informer_handler)
}
