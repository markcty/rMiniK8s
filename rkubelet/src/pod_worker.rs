use anyhow::Result;
use resources::{informer::Store, objects::KubeObject};
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender},
};

use crate::{models::PodUpdate, pod::Pod, PodList, ResyncNotification};

pub struct PodWorker {
    pods: PodList,
    pod_store: Store<KubeObject>,
    work_queue_tx: Sender<PodUpdate>,
    resync_rx: Receiver<ResyncNotification>,
}

impl PodWorker {
    pub fn new(
        pods: PodList,
        pod_store: Store<KubeObject>,
        work_queue_tx: Sender<PodUpdate>,
        resync_rx: Receiver<ResyncNotification>,
    ) -> Self {
        Self {
            pods,
            pod_store,
            work_queue_tx,
            resync_rx,
        }
    }

    pub async fn run(&mut self, mut work_queue: Receiver<PodUpdate>) -> Result<()> {
        tracing::info!("Pod worker started");
        loop {
            select! {
                Some(update) = work_queue.recv() => self.handle_update(update).await,
                _ = self.resync_rx.recv() => self.handle_resync().await,
                else => break
            }
        }
        Ok(())
    }

    async fn handle_update(&mut self, update: PodUpdate) {
        match update {
            PodUpdate::Add(pod) => self.handle_pod_add(pod).await,
            PodUpdate::Update(pod) => self.handle_pod_update(pod).await,
            PodUpdate::Delete(pod) => self.handle_pod_delete(pod).await,
        }
    }

    async fn handle_resync(&mut self) {
        tracing::info!("Start resync...");
        let store = self.pod_store.read().await;
        let mut pods = self.pods.write().await;
        for (_, pod) in store.iter() {
            let name = &pod.metadata.name;
            pods.insert(name.to_owned());
            let result = self
                .work_queue_tx
                .send(PodUpdate::Update(pod.clone()))
                .await;
            if let Err(err) = result {
                tracing::error!(
                    "[resync] Failed to send pod {} to work queue: {:#}",
                    name,
                    err
                );
            }
        }
        drop(store);
        drop(pods);
        tracing::info!("Finished resync");
    }

    async fn handle_pod_add(&mut self, pod: KubeObject) {
        let name = pod.metadata.name.to_owned();
        tracing::info!("Pod added: {}", name);
        let res = Pod::create(pod).await;
        match res {
            Ok(pod) => {
                let res = pod.start().await;
                match res {
                    Ok(_) => {
                        tracing::info!("Pod {} started", name);
                    },
                    Err(err) => {
                        tracing::error!("Pod {} failed to start: {:#}", name, err);
                    },
                }
            },
            Err(err) => {
                tracing::error!("Failed to create pod {}: {:#}", name, err);
            },
        }
        let mut store = self.pods.write().await;
        store.insert(name);
    }

    async fn handle_pod_update(&mut self, pod: KubeObject) {
        let pods = self.pods.read().await;
        let name = pod.metadata.name.to_owned();
        // Pod is not in list, maybe it's being reconciled
        if !pods.contains(&name) {
            return;
        }
        drop(pods);

        let pod = Pod::load(pod);
        match pod {
            Ok(pod) => {
                // Remove pod from pod list so that status manager won't interfere
                let mut pods = self.pods.write().await;
                pods.remove(&name);
                drop(pods);

                let res = pod.reconcile().await;
                match res {
                    Ok(_) => {
                        tracing::info!("Pod {} reconciled", name);
                    },
                    Err(err) => {
                        tracing::error!("Failed to reconcile pod {}: {:#}", name, err);
                    },
                }

                // Put pod back into pod list
                let mut pods = self.pods.write().await;
                pods.insert(name);
                drop(pods);
            },
            Err(err) => {
                tracing::error!("Failed to load pod {}: {:#}", name, err)
            },
        }
    }

    async fn handle_pod_delete(&mut self, pod: KubeObject) {
        let name = pod.metadata.name.to_owned();
        tracing::info!("Pod deleted: {}", name);
        let mut store = self.pods.write().await;
        store.remove(&name);
        let pod = Pod::load(pod);
        match pod {
            Ok(pod) => {
                let res = pod.remove().await;
                match res {
                    Ok(_) => {
                        tracing::info!("Pod {} removed", name);
                    },
                    Err(err) => {
                        tracing::error!("Failed to remove pod {}: {:#}", name, err);
                    },
                }
            },
            Err(err) => tracing::error!("Failed to load pod {}: {:#}", name, err),
        }
    }
}
