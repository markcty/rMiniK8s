use std::sync::Arc;

use anyhow::Result;
use tokio::{sync::Mutex, time::sleep};

use crate::{config::CONFIG, pod_manager::PodManager};

pub struct StatusManager {
    pod_manager: Arc<Mutex<PodManager>>,
}

impl StatusManager {
    pub fn new(pod_manager: Arc<Mutex<PodManager>>) -> Self {
        Self {
            pod_manager,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("Status manager started");
        loop {
            for mut p in self.pod_manager.lock().await.iter_mut() {
                let pod = p.value_mut();
                let changed = pod.update_status().await?;
                if changed {
                    tracing::info!("Pod {} status changed", pod.metadata().name);
                }
            }
            sleep(std::time::Duration::from_secs(
                CONFIG.pod_status_update_frequency,
            ))
            .await;
        }
    }
}
