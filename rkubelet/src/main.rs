use std::{collections::HashSet, sync::Arc};

use anyhow::{anyhow, Error, Result};
use reqwest::Url;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler, WsStream},
    models::Response,
    objects::KubeObject,
};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::connect_async;

use crate::{
    config::CONFIG, models::PodUpdate, pod_worker::PodWorker, status_manager::StatusManager,
};

mod config;
mod docker;
mod models;
mod pod;
mod pod_worker;
mod status_manager;
mod volume;

pub type PodList = Arc<RwLock<HashSet<String>>>;

#[derive(Debug)]
pub struct ResyncNotification;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("rKubelet started");

    let lw = ListerWatcher {
        lister: Box::new(move |_| {
            Box::pin(async {
                let res = reqwest::get(format!("{}/api/v1/pods", CONFIG.cluster.api_server_url))
                    .await?
                    .json::<Response<Vec<KubeObject>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<KubeObject>, Error>(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let url = Url::parse(
                    format!("{}/api/v1/watch/pods", CONFIG.cluster.api_server_watch_url).as_str(),
                )?;
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    };

    // Create work queue and register event handler closures
    let (tx_add, rx) = mpsc::channel::<PodUpdate>(16);
    let tx_update = tx_add.clone();
    let tx_delete = tx_add.clone();
    let eh = EventHandler::<KubeObject> {
        add_cls: Box::new(move |pod| {
            // TODO: this is not good: tx is copied every time add_cls is called, but I can't find a better way
            let tx_add = tx_add.clone();
            Box::pin(async move {
                let message = PodUpdate::Add(pod);
                tx_add.send(message).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |(_old_pod, new_pod)| {
            let tx_update = tx_update.clone();
            Box::pin(async move {
                let message = PodUpdate::Update(new_pod);
                tx_update.send(message).await?;
                Ok(())
            })
        }),
        delete_cls: Box::new(move |old_pod| {
            let tx_delete = tx_delete.clone();
            Box::pin(async move {
                let message = PodUpdate::Delete(old_pod);
                tx_delete.send(message).await?;
                Ok(())
            })
        }),
    };
    let (resync_tx, resync_rx) = mpsc::channel::<ResyncNotification>(16);
    let rh = ResyncHandler(Box::new(move |()| {
        let resync_tx = resync_tx.clone();
        Box::pin(async move {
            resync_tx.send(ResyncNotification).await?;
            Ok(())
        })
    }));

    // Start the informer
    let informer = Informer::new(lw, eh, rh);
    let pod_store = informer.get_store();
    let informer_handle = tokio::spawn(async move { informer.run().await });

    let pods: PodList = Arc::new(RwLock::new(HashSet::new()));
    // Start pod worker
    let mut pod_worker = PodWorker::new(pods.clone(), pod_store.clone(), resync_rx);
    let pod_worker_handle = tokio::spawn(async move { pod_worker.run(rx).await });

    let mut status_manager = StatusManager::new(pods, pod_store);
    let status_manager_handle = tokio::spawn(async move { status_manager.run().await });

    status_manager_handle.await?.expect("Status manager failed");
    pod_worker_handle.await?.expect("Pod worker failed");
    informer_handle.await?
    // TODO: Gracefully shutdown
}
