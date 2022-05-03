use std::sync::Arc;

use anyhow::Result;
use resources::models::Response;
use tokio::{sync::Mutex, time::sleep};

use crate::{config::CONFIG, pod::Pod, pod_manager::PodManager};

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
                let name = pod.metadata().name.to_owned();
                let changed = pod.update_status().await.unwrap_or_else(|err| {
                    tracing::error!("Failed to update status for pod {}: {:#?}", name, err);
                    false
                });
                if changed {
                    tracing::info!("Pod {} status changed", name);
                    let res = self.post_status(pod).await;
                    match res {
                        Ok(_) => {
                            tracing::info!("Posted status for pod {}", name);
                        },
                        Err(err) => {
                            tracing::error!("Failed to post status for {}: {:#?}", name, err);
                        },
                    }
                }
            }
            sleep(std::time::Duration::from_secs(
                CONFIG.pod_status_update_frequency,
            ))
            .await;
        }
    }

    async fn post_status(&self, pod: &Pod) -> Result<()> {
        let client = reqwest::Client::new();
        let payload = pod.object();
        client
            .put(format!(
                "{}/api/v1/pods/{}",
                CONFIG.api_server_url,
                pod.metadata().name
            ))
            .json(&payload)
            .send()
            .await?
            .json::<Response<()>>()
            .await?;
        Ok(())
    }
}
