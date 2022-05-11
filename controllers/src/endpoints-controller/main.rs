use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use resources::objects::KubeObject;
use tokio::sync::mpsc;
use utils::pod_ip_changed;

use crate::utils::{
    add_enpoints, add_svc_endpoint, create_pods_informer, create_services_informer,
    del_svc_endpoint,
};

mod utils;

#[derive(Debug)]
pub enum Notification {
    Pod(PodNtf),
    Service(ServiceNtf),
}

#[derive(Debug)]
pub enum PodNtf {
    Add(KubeObject),
    Update(KubeObject, KubeObject),
    Delete(KubeObject),
}

#[derive(Debug)]
pub enum ServiceNtf {
    Add(KubeObject),
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("Endpoints controller started");

    let (tx, mut rx) = mpsc::channel::<Notification>(16);

    let (pod_informer, pod_store) = create_pods_informer(tx.clone());
    let (svc_informer, svc_store) = create_services_informer(tx.clone());

    let pod_informer_handler = tokio::spawn(async move { pod_informer.run().await });
    let svc_informer_handler = tokio::spawn(async move { svc_informer.run().await });

    while let Some(n) = rx.recv().await {
        if let Err(e) = handle_notification(pod_store.to_owned(), svc_store.to_owned(), n).await {
            tracing::warn!("Error handling notification, caused by: {}", e);
        }
    }

    pod_informer_handler.await??;
    svc_informer_handler.await??;

    Ok(())
}

async fn handle_notification(
    pod_store: Arc<DashMap<String, KubeObject>>,
    svc_store: Arc<DashMap<String, KubeObject>>,
    n: Notification,
) -> Result<()> {
    if let Notification::Pod(n) = n {
        match n {
            PodNtf::Add(new) => {
                add_svc_endpoint(svc_store.to_owned(), new).await?;
            },
            PodNtf::Update(old, new) => {
                if pod_ip_changed(&old, &new) {
                    del_svc_endpoint(svc_store.to_owned(), old).await?;
                    add_svc_endpoint(svc_store.to_owned(), new).await?;
                }
            },
            PodNtf::Delete(old) => {
                del_svc_endpoint(svc_store.to_owned(), old).await?;
            },
        }
    } else if let Notification::Service(ServiceNtf::Add(new)) = n {
        add_enpoints(pod_store.to_owned(), new).await?;
    } else {
    }

    Ok(())
}
