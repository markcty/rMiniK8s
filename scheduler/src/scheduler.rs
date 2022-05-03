use anyhow::{anyhow, Ok, Result};
use resources::{
    models,
    objects::{
        binding::Binding, object_reference::ObjectReference, KubeObject, KubeResource, Metadata,
    },
};
use tokio::sync::mpsc::Receiver;

pub struct Scheduler<T>
where
    T: Fn(&KubeObject, &Vec<KubeObject>) -> ObjectReference,
{
    algorithm: T,
    client: reqwest::Client,
}

impl<T> Scheduler<T>
where
    T: Fn(&KubeObject, &Vec<KubeObject>) -> ObjectReference,
{
    pub fn new(algorithm: T) -> Scheduler<T> {
        Scheduler {
            algorithm,
            client: reqwest::Client::new(),
        }
    }

    pub async fn run(&self, mut pod_queue: Receiver<KubeObject>) -> Result<()> {
        while let Some(pod) = pod_queue.recv().await {
            tracing::info!("schedule pod: {}", pod.name());
            let res = reqwest::get("http://localhost:8080/api/v1/nodes")
                .await?
                .json::<models::Response<Vec<KubeObject>>>()
                .await?;
            let nodes = res.data.ok_or_else(|| anyhow!("list nodes failed"))?;
            let node = (self.algorithm)(&pod, &nodes);
            self.bind(pod, node).await?;
        }
        Ok(())
    }

    async fn bind(&self, pod: KubeObject, node: ObjectReference) -> Result<()> {
        let binding = KubeObject {
            metadata: Metadata {
                name: pod.name(),
                uid: pod.metadata.uid,
            },
            resource: KubeResource::Binding(Binding {
                target: node,
            }),
        };
        let _ = self
            .client
            .post("http://localhost:8080/api/v1/bindings")
            .json(&binding)
            .send()
            .await?
            .json::<models::Response<()>>()
            .await?;

        Ok(())
    }
}
