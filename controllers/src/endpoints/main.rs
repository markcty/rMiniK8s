#[macro_use]
extern crate lazy_static;

use std::{collections::HashSet, env, net::Ipv4Addr};

use anyhow::Result;
use reqwest::Url;
use resources::{
    informer::Store,
    models::NodeConfig,
    objects::{pod::Pod, service::Service},
};
use tokio::{select, sync::mpsc};
use utils::update_service;
lazy_static! {
    static ref CONFIG: NodeConfig = {
        dotenv::from_path("/etc/rminik8s/node.env").ok();
        NodeConfig {
            etcd_endpoint: match env::var("ETCD_ENDPOINT") {
                Ok(url) => Url::parse(url.as_str()).unwrap(),
                Err(_) => Url::parse("http://127.0.0.1:2379/").unwrap(),
            },
            api_server_endpoint: match env::var("API_SERVER_ENDPOINT") {
                Ok(url) => Url::parse(url.as_str()).unwrap(),
                Err(_) => Url::parse("http://127.0.0.1:8080/").unwrap(),
            },
        }
    };
}

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
    Add(Pod),
    Update(Pod, Pod),
    Delete(Pod),
}

#[derive(Debug)]
pub enum ServiceNtf {
    Add(Service),
}

#[derive(Debug)]
pub enum ResyncNtf {
    Pod,
    Svc,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("Endpoints controller started");

    let (tx, mut rx) = mpsc::channel::<Notification>(16);
    let (resync_tx, mut resync_rx) = mpsc::channel::<ResyncNtf>(16);

    let pod_informer = create_pods_informer(tx.clone(), resync_tx.clone());
    let pod_store = pod_informer.get_store();
    let svc_informer = create_services_informer(tx.clone(), resync_tx.clone());
    let svc_store = svc_informer.get_store();

    let pod_informer_handler = tokio::spawn(async move { pod_informer.run().await });
    let svc_informer_handler = tokio::spawn(async move { svc_informer.run().await });

    loop {
        select! {
            _ = resync_rx.recv() => {
                handle_resync(pod_store.to_owned(), svc_store.to_owned()).await?;
            },
            Some(n) = rx.recv() => {
                if let Err(e) = handle_notification(pod_store.to_owned(), svc_store.to_owned(), n).await {
                    tracing::warn!("Error handling notification, caused by: {}", e);
                }
            },
            else => break,
        };
    }

    pod_informer_handler.await??;
    svc_informer_handler.await??;

    Ok(())
}

async fn handle_resync(pod_store: Store<Pod>, svc_store: Store<Service>) -> Result<()> {
    tracing::info!("Resync ...");

    let mut svc_store = svc_store.write().await;
    let pod_store = pod_store.read().await;
    for (_, svc) in svc_store.iter_mut() {
        let svc_spec = &mut svc.spec;
        let mut new_eps: HashSet<Ipv4Addr> = HashSet::new();

        for (_, pod) in pod_store.iter() {
            if let Some(pod_ip) = pod.get_ip() {
                if svc_spec.selector.matches(&pod.metadata.labels) {
                    new_eps.insert(pod_ip);
                }
            }
        }

        if svc_spec.endpoints != new_eps {
            svc_spec.endpoints.clone_from(&new_eps);
            update_service(svc).await?;
        }
    }
    tracing::info!("Resync succeeded!");

    Ok(())
}

async fn handle_notification(
    pod_store: Store<Pod>,
    svc_store: Store<Service>,
    n: Notification,
) -> Result<()> {
    if let Notification::Pod(n) = n {
        match n {
            PodNtf::Add(new) => {
                add_svc_endpoint(svc_store.to_owned(), new).await?;
            },
            PodNtf::Update(old, new) => {
                if old.get_ip() != new.get_ip() {
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
