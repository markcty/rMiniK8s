use anyhow::Result;
use resources::{informer::Store, models::Response, objects::pod};
use tokio::time::sleep;

use crate::{config::CONFIG, pod::Pod, PodList};

pub struct StatusManager {
    pods: PodList,
    pod_store: Store<pod::Pod>,
}

impl StatusManager {
    pub fn new(pods: PodList, pod_store: Store<pod::Pod>) -> Self {
        Self {
            pods,
            pod_store,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("Status manager started");
        loop {
            let store = self.pods.read().await;
            for name in store.iter() {
                let store = self.pod_store.read().await;
                let pod = store.get(&format!("/api/v1/pods/{}", name));
                if pod.is_none() {
                    tracing::warn!("Pod {} not found", name);
                    drop(store);
                    continue;
                }
                let mut pod = Pod::load(pod.unwrap().to_owned())?;
                drop(store);

                let changed = pod.update_status().await.unwrap_or_else(|err| {
                    tracing::error!("Failed to update status for pod {}: {:#?}", name, err);
                    false
                });
                if changed {
                    tracing::info!("Pod {} status changed", name);
                    let res = self.post_status(&pod).await;
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
            drop(store);
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
                CONFIG.cluster.api_server_url,
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
