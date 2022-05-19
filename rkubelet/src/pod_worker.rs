use anyhow::Result;
use resources::{informer::Store, objects::KubeObject};
use tokio::{select, sync::mpsc::Receiver};

use crate::{models::PodUpdate, pod::Pod, PodList, ResyncNotification};

pub struct PodWorker {
    pods: PodList,
    pod_store: Store<KubeObject>,
    resync_rx: Receiver<ResyncNotification>,
}

impl PodWorker {
    pub fn new(
        pods: PodList,
        pod_store: Store<KubeObject>,
        resync_rx: Receiver<ResyncNotification>,
    ) -> Self {
        Self {
            pods,
            pod_store,
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
            PodUpdate::Add(pod) => {
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
                                tracing::error!("Pod {} failed to start: {}", name, err);
                            },
                        }
                    },
                    Err(err) => {
                        tracing::error!("Failed to create pod {}: {:#?}", name, err);
                    },
                }
                let mut store = self.pods.write().await;
                store.insert(name);
            },
            PodUpdate::Update(_) => {},
            PodUpdate::Delete(pod) => {
                let name = pod.metadata.name.to_owned();
                tracing::info!("Pod deleted: {}", name);
                let mut store = self.pods.write().await;
                store.remove(&name);
                // TODO: Should not return directly
                let pod = Pod::load(pod);
                match pod {
                    Ok(pod) => {
                        let res = pod.remove().await;
                        match res {
                            Ok(_) => {
                                tracing::info!("Pod {} removed", pod.metadata().name);
                            },
                            Err(err) => {
                                tracing::error!(
                                    "Failed to remove pod {}: {}",
                                    pod.metadata().name,
                                    err
                                );
                            },
                        }
                    },
                    Err(err) => tracing::error!("Failed to load pod {}: {}", name, err),
                }
            },
        }
    }

    async fn handle_resync(&mut self) {
        let store = self.pod_store.read().await;
        let mut pods = self.pods.write().await;
        store.iter().for_each(|(_, pod)| {
            pods.insert(pod.metadata.name.to_owned());
        });
    }
}
