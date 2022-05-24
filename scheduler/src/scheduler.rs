use anyhow::Result;
use resources::{
    models,
    objects::{binding::Binding, object_reference::ObjectReference, pod::Pod, KubeObject, Object},
};
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender},
};

use crate::{cache::Cache, informer::ResyncNotification, NodeUpdate, PodUpdate};

pub struct Scheduler<T>
where
    T: Fn(&Pod, &Cache) -> Option<ObjectReference>,
{
    cache: Cache,
    algorithm: T,
    client: reqwest::Client,
    resync_rx: Receiver<ResyncNotification>,
    pod_queue_tx: Sender<PodUpdate>,
}

impl<T> Scheduler<T>
where
    T: Fn(&Pod, &Cache) -> Option<ObjectReference>,
{
    pub fn new(
        algorithm: T,
        cache: Cache,
        resync_rx: Receiver<ResyncNotification>,
        pod_queue_tx: Sender<PodUpdate>,
    ) -> Scheduler<T> {
        Scheduler {
            cache,
            algorithm,
            client: reqwest::Client::new(),
            resync_rx,
            pod_queue_tx,
        }
    }

    pub async fn run(
        &mut self,
        mut pod_queue: Receiver<PodUpdate>,
        mut node_queue: Receiver<NodeUpdate>,
    ) {
        tracing::info!("Scheduler started");
        self.cache.refresh().await;

        loop {
            select! {
                Some(update) = pod_queue.recv() => {
                    self.handle_pod_change(update).await;
                },
                Some(update) = node_queue.recv() => {
                    self.handle_node_change(update).await;
                },
                Some(notification) = self.resync_rx.recv() => {
                    match notification {
                        ResyncNotification::EnqueuePods => {
                            let store = self.cache.pod_cache.read().await;
                            for pod in store.values() {
                                if self.pod_queue_tx.send(PodUpdate::Add(pod.to_owned())).await.is_err() {
                                    tracing::error!("Failed to send pod {} to work queue", pod.name());
                                }
                            }
                        },
                        ResyncNotification::RefreshCache => {
                            self.cache.refresh().await;
                        }
                    }
                },
            }
        }
    }

    async fn bind(&self, pod: &Pod, node: ObjectReference) -> Result<()> {
        let binding = KubeObject::Binding(Binding {
            metadata: pod.metadata.clone(),
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

    async fn handle_pod_change(&mut self, update: PodUpdate) {
        match update {
            PodUpdate::Add(pod) => self.handle_pod_add(pod).await,
            PodUpdate::Update(..) => {},
            PodUpdate::Delete(pod) => self.cache.handle_pod_delete(pod).await,
        };
    }

    async fn handle_node_change(&mut self, update: NodeUpdate) {
        match update {
            NodeUpdate::Add(node) => self.cache.handle_node_add(node).await,
            NodeUpdate::Update(_, new_node) => self.cache.handle_node_update(new_node).await,
            NodeUpdate::Delete(node) => self.cache.handle_node_delete(node).await,
        };
    }

    async fn handle_pod_add(&mut self, pod: Pod) {
        let pod_name = pod.name().to_owned();
        tracing::info!("Scheduling pod: {}", pod_name);
        if pod.spec.node_name.is_some() {
            tracing::debug!("Pod {} already scheduled", pod_name);
            return;
        }
        // Schedule pod
        let node = (self.algorithm)(&pod, &self.cache);
        match node {
            Some(node) => {
                let node_name = node.name.to_owned();
                match self.bind(&pod, node).await {
                    Ok(()) => {
                        self.cache.handle_pod_add(pod, &node_name).await;
                        tracing::info!("Pod {} scheduled to node {}", pod_name, node_name)
                    },
                    Err(e) => tracing::error!(
                        "Failed to bind pod {} to node {}: {:#}",
                        pod_name,
                        node_name,
                        e
                    ),
                }
            },
            None => tracing::warn!("No schedulable node found for pod {}", pod_name),
        }
    }
}
