use anyhow::{Ok, Result};
use resources::{
    models,
    objects::{
        binding::Binding, object_reference::ObjectReference, KubeObject, KubeResource, Metadata,
    },
};
use tokio::sync::mpsc::Receiver;

pub struct Scheduler<T>
where
    T: Fn(&KubeObject) -> KubeObject,
{
    algorithm: T,
    client: reqwest::Client,
}

impl<T> Scheduler<T>
where
    T: Fn(&KubeObject) -> KubeObject,
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
            let node = (self.algorithm)(&pod);
            self.bind(pod, node).await?;
        }
        Ok(())
    }

    async fn bind(&self, pod: KubeObject, node: KubeObject) -> Result<()> {
        let binding = KubeObject {
            metadata: Metadata {
                name: pod.name(),
                uid: pod.metadata.uid,
            },
            resource: KubeResource::Binding(Binding {
                target: ObjectReference {
                    kind: "node".to_string(),
                    name: node.name(),
                },
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
