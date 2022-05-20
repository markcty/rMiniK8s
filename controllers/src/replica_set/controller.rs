use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use anyhow::{Context, Error, Result};
use resources::{
    informer::Store,
    models::Response,
    objects::{
        object_reference::ObjectReference,
        pod::{Pod, PodPhase},
        replica_set::{ReplicaSet, ReplicaSetStatus},
        KubeObject, KubeResource,
    },
};
use tokio::{
    select,
    sync::{mpsc, mpsc::Receiver},
    task::JoinHandle,
};

use crate::{
    utils::{create_informer, unwrap_pod, unwrap_rs, unwrap_rs_mut, Event, ResyncNotification},
    CONFIG,
};

pub struct ReplicaSetController {
    rx: Receiver<Event>,

    rs_resync_rx: Receiver<ResyncNotification>,
    rs_informer: Option<JoinHandle<Result<(), Error>>>,
    rs_store: Store<KubeObject>,

    pod_resync_rx: Receiver<ResyncNotification>,
    pod_informer: Option<JoinHandle<Result<(), Error>>>,
    pod_store: Store<KubeObject>,
}

impl ReplicaSetController {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Event>(16);

        let (rs_resync_tx, rs_resync_rx) = mpsc::channel::<ResyncNotification>(16);
        let rs_informer = create_informer("replicasets".to_string(), tx.clone(), rs_resync_tx);
        let rs_store = rs_informer.get_store();

        let (pod_resync_tx, pod_resync_rx) = mpsc::channel::<ResyncNotification>(16);
        let pod_informer = create_informer("pods".to_string(), tx, pod_resync_tx);
        let pod_store = pod_informer.get_store();

        let rs_informer = tokio::spawn(async move { rs_informer.run().await });
        let pod_informer = tokio::spawn(async move { pod_informer.run().await });

        Self {
            rx,

            rs_resync_rx,
            rs_informer: Some(rs_informer),
            rs_store,

            pod_resync_rx,
            pod_informer: Some(pod_informer),
            pod_store,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("ReplicaSet Controller started");

        loop {
            select! {
                Some(event) = self.rx.recv() => {
                    let result = match event {
                        Event::Add(object) => match object.resource {
                            KubeResource::ReplicaSet(_) => self.reconcile(&object).await,
                            KubeResource::Pod(_) => self.handle_pod_change(object).await,
                            _ => Err(anyhow::anyhow!("Unexpected resource {}", object.kind())),
                        },
                        Event::Update(_, new_object) => match new_object.resource {
                            KubeResource::ReplicaSet(_) => self.reconcile(&new_object).await,
                            KubeResource::Pod(_) => self.handle_pod_change(new_object).await,
                            _ => Err(anyhow::anyhow!("Unexpected resource {}", new_object.kind())),
                        },
                        Event::Delete(mut object) => match object.resource {
                            KubeResource::ReplicaSet(ref mut rs) => {
                                rs.spec.replicas = 0;
                                self.reconcile(&object).await
                            },
                            KubeResource::Pod(_) => self.handle_pod_change(object).await,
                            _ => Err(anyhow::anyhow!("Unexpected resource {}", object.kind())),
                        },
                    };
                    if let Err(e) = result {
                        tracing::error!("Error while processing: {:#?}", e);
                    }
                },
                _ = self.rs_resync_rx.recv() => self.resync_rs().await,
                _ = self.pod_resync_rx.recv() => self.resync_pod().await,
                else => break
            }
        }

        let rs_informer = std::mem::replace(&mut self.rs_informer, None);
        rs_informer.unwrap().await??;
        let pod_informer = std::mem::replace(&mut self.pod_informer, None);
        pod_informer.unwrap().await??;
        tracing::info!("ReplicaSet Controller exited");
        Ok(())
    }

    async fn handle_pod_change(&self, object: KubeObject) -> Result<()> {
        let name = object.metadata.name.to_owned();
        let owner = self.resolve_owner(&object.metadata.owner_references).await;
        match owner {
            Some(ref owner) => {
                self.update_replicaset(owner).await?;
            },
            None => tracing::debug!("Pod {} is not owned by any ReplicaSet", name),
        };
        Ok(())
    }

    async fn update_replicaset(&self, object: &KubeObject) -> Result<()> {
        let mut object = object.to_owned();
        let name = object.metadata.name.to_owned();
        let mut rs = unwrap_rs_mut(&mut object);
        let new_status = self.calculate_status(rs).await;
        let status = rs
            .status
            .as_mut()
            .with_context(|| format!("ReplicaSet {} has no status", name))?;
        if !new_status.eq(status) {
            tracing::info!(
                "Observed change in ReplicaSet {}, replicas: {} -> {}, ready_replicas: {} -> {}",
                name,
                status.replicas,
                new_status.replicas,
                status.ready_replicas,
                new_status.ready_replicas
            );
            rs.status = Some(new_status);
            self.post_status(&object).await?;
        }
        Ok(())
    }

    async fn reconcile(&self, object: &KubeObject) -> Result<()> {
        let rs = unwrap_rs(object);
        let status = rs
            .status
            .as_ref()
            .with_context(|| "ReplicaSet has no status")?;
        tracing::info!(
            "Tuning ReplicaSet {}, desired: {} current: {}",
            object.metadata.name,
            rs.spec.replicas,
            status.replicas,
        );
        let current = status.replicas;
        let desired = rs.spec.replicas;
        match current.cmp(&desired) {
            Ordering::Less => {
                // Create a new pod, if more pods are needed, they'll be created later
                self.create_pod(object).await?;
            },
            Ordering::Greater => {
                // Delete existing pods
                let pod_name = self.get_pod_to_delete(rs).await;
                self.delete_pod(pod_name).await?;
            },
            Ordering::Equal => {
                // Nothing to do
            },
        }
        Ok(())
    }

    async fn post_status(&self, rs: &KubeObject) -> Result<()> {
        let client = reqwest::Client::new();
        let response = client
            .put(format!(
                "{}/api/v1/replicasets/{}",
                CONFIG.api_server_url, rs.metadata.name
            ))
            .json(&rs)
            .send()
            .await?
            .json::<Response<()>>()
            .await
            .with_context(|| "Error posting status")?;
        tracing::info!(
            "Posted status for ReplicaSet {}: {}",
            rs.metadata.name,
            response.msg.unwrap_or_else(|| "".to_string())
        );
        Ok(())
    }

    async fn create_pod(&self, object: &KubeObject) -> Result<()> {
        let rs = unwrap_rs(object);
        let client = reqwest::Client::new();
        let template = &rs.spec.template;
        let mut metadata = template.metadata.clone();
        metadata.owner_references.push(ObjectReference {
            kind: "ReplicaSet".to_string(),
            name: object.metadata.name.to_owned(),
        });
        let pod = KubeObject {
            metadata,
            resource: KubeResource::Pod(Pod {
                spec: template.spec.clone(),
                status: None,
            }),
        };
        let response = client
            .post(format!("{}/api/v1/pods", CONFIG.api_server_url))
            .json(&pod)
            .send()
            .await?
            .json::<Response<()>>()
            .await
            .with_context(|| "Error creating pod")?;
        if let Some(msg) = response.msg {
            tracing::info!("{}", msg);
        }
        Ok(())
    }

    async fn delete_pod(&self, name: String) -> Result<()> {
        let client = reqwest::Client::new();
        let response = client
            .delete(format!("{}/api/v1/pods/{}", CONFIG.api_server_url, name))
            .send()
            .await?
            .json::<Response<()>>()
            .await
            .with_context(|| "Error deleting pod")?;
        if let Some(msg) = response.msg {
            tracing::info!("{}", msg);
        }
        Ok(())
    }

    async fn resolve_owner(&self, refs: &[ObjectReference]) -> Option<KubeObject> {
        let owners: Vec<&ObjectReference> = refs
            .iter()
            .filter(|r| r.kind == "ReplicaSet")
            .collect::<Vec<_>>();
        if owners.len() != 1 {
            tracing::debug!("Pod doesn't have exactly one owner");
            return None;
        }
        // Clone the object and drop the reference,
        // otherwise informer may deadlock when handling watch event
        let store = self.rs_store.read().await;
        let res = store.get(format!("/api/v1/replicasets/{}", owners[0].name).as_str());
        res.cloned()
    }

    async fn get_pods(&self, rs: &ReplicaSet) -> Vec<KubeObject> {
        let store = self.pod_store.read().await;
        store
            .iter()
            .filter(|(_, pod)| pod.metadata.labels.matches(&rs.spec.selector))
            .map(|(_, pod)| pod.to_owned())
            .collect::<Vec<_>>()
    }

    async fn calculate_status(&self, rs: &ReplicaSet) -> ReplicaSetStatus {
        let pods = self.get_pods(rs).await;
        let replicas = pods.len() as u32;
        let mut ready_replicas = 0;
        pods.iter().for_each(|pod| {
            let pod = unwrap_pod(pod);
            if pod.is_ready() {
                ready_replicas += 1;
            }
        });
        ReplicaSetStatus {
            replicas,
            ready_replicas,
        }
    }

    async fn get_pod_to_delete(&self, rs: &ReplicaSet) -> String {
        let mut pods = self.get_pods(rs).await;
        pods.sort_by(|a, b| {
            let a = unwrap_pod(a);
            let b = unwrap_pod(b);
            let status_a = a.status.as_ref().unwrap();
            let status_b = b.status.as_ref().unwrap();
            if status_a.phase != status_b.phase {
                let order = HashMap::from([
                    (PodPhase::Failed, 0),
                    (PodPhase::Pending, 1),
                    (PodPhase::Running, 2),
                    (PodPhase::Succeeded, 3),
                ]);
                return order
                    .get(&status_a.phase)
                    .unwrap()
                    .cmp(order.get(&status_b.phase).unwrap());
            }
            // Not Ready < Ready
            if a.is_ready() != b.is_ready() {
                return a.is_ready().cmp(&b.is_ready());
            }
            if status_a.start_time != status_b.start_time {
                return status_a.start_time.cmp(&status_b.start_time);
            }
            Ordering::Equal
        });
        pods[0].metadata.name.to_owned()
    }

    async fn resync_rs(&self) {
        let store = self.rs_store.read().await;
        for rs in store.values() {
            let result = self.update_replicaset(rs).await;
            if let Err(e) = result {
                tracing::error!("Failed to update ReplicaSet {}: {:#?}", rs.metadata.name, e);
            }
        }
    }

    async fn resync_pod(&self) {
        let mut rs_to_resync = HashSet::<String>::new();
        let store = self.pod_store.read().await;
        for pod in store.values() {
            let owner = self.resolve_owner(&pod.metadata.owner_references).await;
            match owner {
                Some(owner) => {
                    if !rs_to_resync.contains(&owner.metadata.name) {
                        rs_to_resync.insert(owner.metadata.name.to_owned());
                        let result = self.update_replicaset(&owner).await;
                        if let Err(e) = result {
                            tracing::error!(
                                "Failed to update ReplicaSet {}: {:#?}",
                                owner.metadata.name,
                                e
                            );
                        }
                    }
                },
                None => tracing::debug!("Pod {} is not owned by any ReplicaSet", pod.metadata.name),
            };
        }
    }
}
