use anyhow::{Ok, Result};
use resources::{
    models,
    objects::{
        binding::Binding, object_reference::ObjectReference, pod::Pod, KubeObject, Metadata, Object,
    },
};
use tokio::sync::mpsc::Receiver;

use crate::cache::Cache;

pub struct Scheduler<T>
where
    T: Fn(&Pod, &Cache) -> ObjectReference,
{
    cache: Cache,
    algorithm: T,
    client: reqwest::Client,
}

impl<T> Scheduler<T>
where
    T: Fn(&Pod, &Cache) -> ObjectReference,
{
    pub fn new(algorithm: T, cache: Cache) -> Scheduler<T> {
        Scheduler {
            cache,
            algorithm,
            client: reqwest::Client::new(),
        }
    }

    pub async fn run(&self, mut pod_queue: Receiver<Pod>) -> Result<()> {
        tracing::info!("scheduler started");

        while let Some(pod) = pod_queue.recv().await {
            tracing::info!("schedule pod: {}", pod.name());
            let node = (self.algorithm)(&pod, &self.cache);
            self.bind(pod, node).await?;
        }

        Ok(())
    }

    async fn bind(&self, pod: Pod, node: ObjectReference) -> Result<()> {
        let binding = KubeObject::Binding(Binding {
            metadata: Metadata {
                name: pod.name().to_owned(),
                uid: pod.metadata.uid,
                labels: pod.metadata.labels,
                ..Default::default()
            },
            target: node,
        });

        let res = self
            .client
            .post("http://localhost:8080/api/v1/bindings")
            .json(&binding)
            .send()
            .await?;

        if let Err(err) = res.error_for_status_ref() {
            let err_res = res.json::<models::ErrResponse>().await?;
            tracing::error!(
                "status {}, msg: {}, cause: {}",
                err.status().unwrap_or_default(),
                err_res.msg,
                err_res.cause.unwrap_or_else(|| "None".to_string())
            );
        }

        Ok(())
    }
}
