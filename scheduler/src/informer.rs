use anyhow::{anyhow, Error};
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler, Store, WsStream},
    models,
    objects::{node::Node, pod::Pod},
};
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};
use tokio_tungstenite::connect_async;

use crate::{NodeUpdate, PodUpdate, CONFIG};

pub type RunInformerResult<T, E> = (
    Sender<E>,
    Receiver<E>,
    Store<T>,
    JoinHandle<Result<(), Error>>,
);

#[derive(Debug)]
pub enum ResyncNotification {
    /// Enqueue all pods in store
    EnqueuePods,
    RefreshCache,
}

pub fn run_pod_informer(
    resync_tx: Sender<ResyncNotification>,
) -> RunInformerResult<Pod, PodUpdate> {
    // create list watcher closures
    // TODO: maybe some crate or macros can simplify the tedious boxed closure creation in heap
    let lw = ListerWatcher::<Pod> {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get(CONFIG.api_server_endpoint.join("api/v1/pods")?)
                    .await?
                    .json::<models::Response<Vec<Pod>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<Pod>, Error>(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let mut url = CONFIG.api_server_endpoint.join("api/v1/watch/pods")?;
                url.set_scheme("ws").ok();
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    };

    // create event handler closures
    let (tx, rx) = mpsc::channel::<PodUpdate>(16);
    let tx_add = tx.clone();
    let tx_update = tx.clone();
    let tx_delete = tx.clone();
    let eh = EventHandler::<Pod> {
        add_cls: Box::new(move |pod| {
            // TODO: this is not good: tx is copied every time add_cls is called, but I can't find a better way
            let tx_add = tx_add.clone();
            Box::pin(async move {
                tx_add.send(PodUpdate::Add(pod)).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old, new)| {
            let tx_update = tx_update.clone();
            Box::pin(async move {
                tx_update.send(PodUpdate::Update(old, new)).await?;
                Ok(())
            })
        }),
        delete_cls: Box::new(move |pod| {
            let tx_delete = tx_delete.clone();
            Box::pin(async move {
                tx_delete.send(PodUpdate::Delete(pod)).await?;
                Ok(())
            })
        }),
    };

    let rh = ResyncHandler(Box::new(move |_| {
        let resync_tx = resync_tx.clone();
        Box::pin(async move {
            // enqueue all pods
            tracing::debug!("Resyncing pods...");
            resync_tx.send(ResyncNotification::EnqueuePods).await?;
            Ok(())
        })
    }));
    // start the informer
    let informer = Informer::new(lw, eh, rh);
    let store = informer.get_store();
    let informer_handler = tokio::spawn(async move { informer.run().await });

    (tx, rx, store, informer_handler)
}

pub fn run_node_informer(
    resync_tx: Sender<ResyncNotification>,
) -> RunInformerResult<Node, NodeUpdate> {
    // create list watcher closures
    // TODO: maybe some crate or macros can simplify the tedious boxed closure creation in heap
    let lw = ListerWatcher::<Node> {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get(CONFIG.api_server_endpoint.join("api/v1/nodes")?)
                    .await?
                    .json::<models::Response<Vec<Node>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<Node>, Error>(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let mut url = CONFIG.api_server_endpoint.join("api/v1/watch/nodes")?;
                url.set_scheme("ws").ok();
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    };

    // create event handler closures
    let (tx, rx) = mpsc::channel::<NodeUpdate>(16);
    let tx_add = tx.clone();
    let tx_update = tx_add.clone();
    let tx_delete = tx_add.clone();
    let eh = EventHandler::<Node> {
        add_cls: Box::new(move |node| {
            // TODO: this is not good: tx is copied every time add_cls is called, but I can't find a better way
            let tx_add = tx_add.clone();
            Box::pin(async move {
                tx_add.send(NodeUpdate::Add(node)).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old, new)| {
            let tx_update = tx_update.clone();
            Box::pin(async move {
                tx_update.send(NodeUpdate::Update(old, new)).await?;
                Ok(())
            })
        }),
        delete_cls: Box::new(move |node| {
            let tx_delete = tx_delete.clone();
            Box::pin(async move {
                tx_delete.send(NodeUpdate::Delete(node)).await?;
                Ok(())
            })
        }),
    };

    let rh = ResyncHandler(Box::new(move |_| {
        let resync_tx = resync_tx.clone();
        Box::pin(async move {
            tracing::debug!("Resyncing nodes...");
            resync_tx.send(ResyncNotification::RefreshCache).await?;
            Ok(())
        })
    }));
    // start the informer
    let informer = Informer::new(lw, eh, rh);
    let store = informer.get_store();
    let informer_handler = tokio::spawn(async move { informer.run().await });

    (tx, rx, store, informer_handler)
}
