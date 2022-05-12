use std::sync::Arc;

use actix::{prelude::*, Actor};
use anyhow::{anyhow, Error, Result};
use dashmap::DashMap;
use reqwest::Url;
use resources::{
    informer::{Event, EventHandler, Informer, ListerWatcher, WsStream},
    models::Response,
    objects::KubeObject,
};
use tokio::sync::{
    mpsc::{self, Sender},
    Mutex,
};
use tokio_tungstenite::connect_async;

use crate::{
    config::CONFIG, models::PodUpdate, pod_manager::PodManager, pod_worker::PodWorker,
    status_manager::StatusManager,
};

mod config;
mod docker;
mod models;
mod pod;
mod pod_manager;
mod pod_worker;
mod status_manager;
mod volume;

struct PodUpdateHandler {
    tx: Arc<Sender<PodUpdate>>,
}

impl Actor for PodUpdateHandler {
    type Context = Context<Self>;
}

impl Handler<Event<KubeObject>> for PodUpdateHandler {
    type Result = Result<()>;

    fn handle(&mut self, msg: Event<KubeObject>, ctx: &mut Self::Context) -> Self::Result {
        let tx = self.tx.clone();
        let future = Box::pin(async move {
            let message = match msg {
                Event::Add(pod) => PodUpdate::Add(pod),
                Event::Update(_old_pod, new_pod) => PodUpdate::Update(new_pod),
                Event::Delete(old_pod) => PodUpdate::Delete(old_pod),
            };
            let result = tx.send(message).await;
            match result {
                Ok(_) => (),
                Err(e) => {
                    tracing::error!("Failed to send message: {}", e);
                },
            }
        })
        .into_actor(self);
        ctx.spawn(future);
        Ok(())
    }
}

impl EventHandler<KubeObject> for PodUpdateHandler {}

#[actix_rt::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("rKubelet started");

    let lw = ListerWatcher {
        lister: Box::new(move |_| {
            Box::pin(async {
                let res = reqwest::get(format!("{}/api/v1/pods", CONFIG.api_server_url))
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
                    format!("{}/api/v1/watch/pods", CONFIG.api_server_watch_url).as_str(),
                )?;
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    };

    // Create work queue and register event handler closures
    let (tx, rx) = mpsc::channel::<PodUpdate>(16);
    let eh = PodUpdateHandler {
        tx: Arc::new(tx),
    }
    .start();

    // Start the informer
    let store = Arc::new(DashMap::new());
    let informer = Informer::new(lw, eh, store);
    let informer_handle = tokio::spawn(async move { informer.run().await });

    let pm = Arc::new(Mutex::new(PodManager::new()));
    let pm_sm = Arc::clone(&pm);

    // Start pod worker
    let mut pod_worker = PodWorker::new(pm);
    let pod_worker_handle = tokio::spawn(async move { pod_worker.run(rx).await });

    let mut status_manager = StatusManager::new(pm_sm);
    let status_manager_handle = tokio::spawn(async move { status_manager.run().await });

    status_manager_handle.await?.expect("Status manager failed");
    pod_worker_handle.await?.expect("Pod worker failed");
    informer_handle.await?
    // TODO: Gracefully shutdown
}
