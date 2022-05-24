use std::{collections::HashSet, sync::Arc};

use anyhow::{anyhow, Error, Result};
use axum::{routing::get, Extension, Router};
use reqwest::Url;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler, Store, WsStream},
    models::Response,
    objects::pod::Pod,
};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::connect_async;

use crate::{
    config::CONFIG, models::PodUpdate, node_status_manager::NodeStatusManager,
    pod_worker::PodWorker, status_manager::StatusManager,
};

mod api;
mod config;
mod docker;
mod models;
mod node_status_manager;
mod pod;
mod pod_worker;
mod status_manager;
mod volume;

pub type PodList = Arc<RwLock<HashSet<String>>>;

#[derive(Debug)]
pub struct ResyncNotification;

pub struct AppState {
    pod_store: Store<Pod>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("rKubelet started");

    let lw = ListerWatcher {
        lister: Box::new(move |_| {
            Box::pin(async {
                let res = reqwest::get(format!("{}/api/v1/pods", CONFIG.cluster.api_server_url))
                    .await?
                    .json::<Response<Vec<Pod>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<Pod>, Error>(res)
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
    let (tx, rx) = mpsc::channel::<PodUpdate>(16);
    let tx_add = tx.clone();
    let tx_update = tx_add.clone();
    let tx_delete = tx_add.clone();
    let eh = EventHandler::<Pod> {
        add_cls: Box::new(move |pod| {
            // TODO: this is not good: tx is copied every time add_cls is called, but I can't find a better way
            let tx_add = tx_add.clone();
            Box::pin(async move {
                let message = PodUpdate::Add(pod);
                tx_add.send(message).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old_pod, new_pod)| {
            let tx_update = tx_update.clone();
            Box::pin(async move {
                let message = PodUpdate::Update(old_pod, new_pod);
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

    let mut node_status_manager = NodeStatusManager::new();
    let node_name = node_status_manager.node_name();
    let node_status_manager_handle = tokio::spawn(async move { node_status_manager.run().await });

    let pods: PodList = Arc::new(RwLock::new(HashSet::new()));
    // Start pod worker
    let mut pod_worker = PodWorker::new(node_name, pods.clone(), pod_store.clone(), tx, resync_rx);
    let pod_worker_handle = tokio::spawn(async move { pod_worker.run(rx).await });

    let mut status_manager = StatusManager::new(pods, pod_store.clone());
    let status_manager_handle = tokio::spawn(async move { status_manager.run().await });

    // Configure and start rkubelet API server
    let app_state = Arc::new(AppState {
        pod_store,
    });

    let app = Router::new()
        .nest(
            "/api/v1/pods/:pod_name",
            Router::new().route("/logs", get(api::pod_logs)).nest(
                "/containers/:container_name",
                Router::new()
                    .route("/logs", get(api::container_logs))
                    .route("/exec", get(api::container_exec)),
            ),
        )
        .layer(Extension(app_state));

    let addr = format!("0.0.0.0:{}", config::CONFIG.port);
    tracing::info!("Listening at {}", addr);
    axum::Server::bind(&addr.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();

    node_status_manager_handle.await??;
    status_manager_handle.await??;
    pod_worker_handle.await??;
    informer_handle.await?
    // TODO: Gracefully shutdown
}
