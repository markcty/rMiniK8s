#[macro_use]
extern crate lazy_static;

use std::env;

use anyhow::Result;
use reqwest::Url;
use resources::{informer::Store, models::NodeConfig, objects::KubeObject};
use tokio::sync::mpsc;
use utils::pod_ip_changed;

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

    let pod_informer = create_pods_informer(tx.clone());
    let pod_store = pod_informer.get_store();
    let svc_informer = create_services_informer(tx.clone());
    let svc_store = svc_informer.get_store();

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
    pod_store: Store<KubeObject>,
    svc_store: Store<KubeObject>,
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
