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
                    tracing::info!("Pod added: {}", pod.metadata.name);
                    let pod = Pod::create(pod).await?;
                    pod.start().await?;
                    let mut pod_manager = self.pod_manager.lock().await;
                    pod_manager.add_pod(pod);
                },
                PodUpdate::Update(_) => {},
                PodUpdate::Delete(pod) => {
                    let name = pod.metadata.name;
                    tracing::info!("Pod deleted: {}", name);
                    // Avoid immutable and mutable borrow conflict
                    {
                        let pod_manager = self.pod_manager.lock().await;
                        let pod = pod_manager.get_pod(name.as_str());
                        if let Some(pod) = pod {
                            pod.remove().await?;
                        } else {
                            tracing::warn!("Pod not found: {}", name);
                        }
                    }
                    let mut pod_manager = self.pod_manager.lock().await;
                    pod_manager.remove_pod(name.as_str());
                },
            }
        }
        Ok(())
    }
}
