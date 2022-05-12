use std::sync::Arc;

use actix::{prelude::*, Actor};
use anyhow::Result;
use dashmap::DashMap;
use resources::{
    informer::{Event, EventHandler, Informer},
    objects::KubeObject,
};
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::utils::create_lister;

struct ReplicaSetEventHandler {
    tx: Arc<Sender<KubeObject>>,
}

impl Actor for ReplicaSetEventHandler {
    type Context = Context<Self>;
}

impl Handler<Event<KubeObject>> for ReplicaSetEventHandler {
    type Result = Result<()>;

    fn handle(&mut self, msg: Event<KubeObject>, ctx: &mut Self::Context) -> Self::Result {
        let tx = self.tx.clone();
        let future = Box::pin(async move {
            let message = match msg {
                Event::Add(rs) => rs,
                Event::Update(_old_rs, new_rs) => new_rs,
                Event::Delete(old_rs) => old_rs,
            };
            let result = tx.send(message).await;
            match result {
                Ok(_) => (),
                Err(e) => {
                    tracing::error!("Failed to send message: {}", e);
                },
            }
        })
        .into_actor(self);
        ctx.spawn(future);
        Ok(())
    }
}

impl EventHandler<KubeObject> for ReplicaSetEventHandler {}

struct PodEventHandler {
    tx: Arc<Sender<KubeObject>>,
    store: Arc<DashMap<String, KubeObject>>,
}

impl Actor for PodEventHandler {
    type Context = Context<Self>;
}

impl Handler<Event<KubeObject>> for PodEventHandler {
    type Result = Result<()>;

    fn handle(&mut self, msg: Event<KubeObject>, ctx: &mut Self::Context) -> Self::Result {
        let tx = self.tx.clone();
        let future = Box::pin(async move {
            let message = match msg {
                Event::Add(rs) => rs,
                Event::Update(_old_rs, new_rs) => new_rs,
                Event::Delete(old_rs) => old_rs,
            };
            let result = tx.send(message).await;
            match result {
                Ok(_) => (),
                Err(e) => {
                    tracing::error!("Failed to send message: {}", e);
                },
            }
        })
        .into_actor(self);
        ctx.spawn(future);
        Ok(())
    }
}

impl EventHandler<KubeObject> for PodEventHandler {}

pub struct ReplicaSetController {
    tx: Arc<Sender<KubeObject>>,
    rx: Receiver<KubeObject>,
    rs_informer: Option<Informer<KubeObject, ReplicaSetEventHandler>>,
    rs_store: Option<Arc<DashMap<String, KubeObject>>>,
    pod_informer: Option<Informer<KubeObject, PodEventHandler>>,
    pod_store: Option<Arc<DashMap<String, KubeObject>>>,
}

#[allow(dead_code)]
impl ReplicaSetController {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<KubeObject>(16);
        Self {
            tx: Arc::new(tx),
            rx,
            rs_informer: None,
            rs_store: None,
            pod_informer: None,
            pod_store: None,
        }
    }

    pub fn run(&mut self) {
        let (rs_informer, rs_store) = self.create_rs_informer();
        let (pod_informer, pod_store) = self.create_pod_informer();
        self.rs_informer = Some(rs_informer);
        self.rs_store = Some(rs_store);
        self.pod_informer = Some(pod_informer);
        self.pod_store = Some(pod_store);
    }

    pub fn create_rs_informer(
        &self,
    ) -> (
        Informer<KubeObject, ReplicaSetEventHandler>,
        Arc<DashMap<String, KubeObject>>,
    ) {
        let lw = create_lister("replicasets");

        // Create work queue and register event handler closures
        let eh = ReplicaSetEventHandler {
            tx: Arc::clone(&self.tx),
        }
        .start();

        // Start the informer
        let store = Arc::new(DashMap::new());
        let informer = Informer::new(lw, eh, store.clone());

        (informer, store)
    }

    pub fn create_pod_informer(
        &self,
    ) -> (
        Informer<KubeObject, PodEventHandler>,
        Arc<DashMap<String, KubeObject>>,
    ) {
        let lw = create_lister("pods");

        // Create work queue and register event handler closures
        let rs_store = self
            .rs_store
            .as_ref()
            .expect("ReplicaSet store not initialized");
        let eh = PodEventHandler {
            tx: Arc::clone(&self.tx),
            store: Arc::clone(rs_store),
        }
        .start();

        // Start the informer
        let store = Arc::new(DashMap::new());
        let informer = Informer::new(lw, eh, store.clone());

        (informer, store)
    }

    fn add_pod(&self, pod: &KubeObject) {
        tracing::info!("Add pod {}", pod.metadata.name);
    }

    fn update_pod(&self, pod: &KubeObject) {
        tracing::info!("Update pod {}", pod.metadata.name);
    }

    fn delete_pod(&self, pod: &KubeObject) {
        tracing::info!("Remove pod {}", pod.metadata.name);
    }
}
