use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc::Receiver, Mutex};

use crate::{models::PodUpdate, pod::Pod, pod_manager::PodManager};

pub struct PodWorker {
    pod_manager: Arc<Mutex<PodManager>>,
}

impl PodWorker {
    pub fn new(pod_manager: Arc<Mutex<PodManager>>) -> Self {
        Self {
            pod_manager,
        }
    }

    pub async fn run(&mut self, mut work_queue: Receiver<PodUpdate>) -> Result<()> {
        tracing::info!("Pod worker started");
        while let Some(update) = work_queue.recv().await {
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
                            let mut pod_manager = self.pod_manager.lock().await;
                            pod_manager.add_pod(pod);
                        },
                        Err(err) => {
                            tracing::error!("Failed to create pod {}: {:#?}", name, err);
                        },
                    }
                },
                PodUpdate::Update(_) => {},
                PodUpdate::Delete(pod) => {
                    let name = pod.metadata.name;
                    tracing::info!("Pod deleted: {}", name);
                    let mut pm = self.pod_manager.lock().await;
                    // Avoid immutable and mutable borrow conflict
                    {
                        let pod = pm.get_pod(name.as_str());
                        if let Some(pod) = pod {
                            let res = pod.remove().await;
                            match res {
                                Ok(_) => {
                                    tracing::info!("Pod {} removed", name);
                                },
                                Err(err) => {
                                    tracing::error!("Failed to remove pod {}: {}", name, err);
                                },
                            }
                        } else {
                            tracing::warn!("Pod not found: {}", name);
                        }
                    }
                    pm.remove_pod(name.as_str());
                },
            }
        }
        Ok(())
    }
}
