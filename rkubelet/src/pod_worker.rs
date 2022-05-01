use anyhow::Result;
use tokio::sync::mpsc::Receiver;

use crate::{models::PodUpdate, pod::Pod};

pub struct PodWorker {}

impl PodWorker {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(&self, mut work_queue: Receiver<PodUpdate>) -> Result<()> {
        tracing::info!("Pod worker started");
        while let Some(update) = work_queue.recv().await {
            match update {
                PodUpdate::Add(pod) => {
                    tracing::info!("Pod added: {}", pod.metadata.name);
                    let pod = Pod::create(pod).await?;
                    pod.start().await?;
                },
                PodUpdate::Update(_) => {},
                PodUpdate::Delete(pod) => {
                    tracing::info!("Pod deleted: {}", pod.metadata.name);
                },
            }
        }
        Ok(())
    }
}
