#[macro_use]
extern crate lazy_static;

use std::env;

use anyhow::Result;
use k8s_iptables::K8sIpTables;
use reqwest::Url;
use resources::{models::NodeConfig, objects::KubeObject};
use tokio::sync::mpsc;

use crate::utils::create_services_informer;

mod k8s_iptables;
mod utils;

#[derive(Debug)]
pub enum Notification {
    Add(KubeObject),
    Update(KubeObject, KubeObject),
    Delete(KubeObject),
}

lazy_static! {
    static ref CONFIG: NodeConfig = {
        dotenv::from_path("/etc/rminik8s/node.env").ok();
        NodeConfig {
            etcd_endpoint: match env::var("ETCD_ENDPOING") {
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("rKube-Proxy started");

    let mut ipt = K8sIpTables::new();

    let (tx, mut rx) = mpsc::channel::<Notification>(16);

    let (svc_informer, _) = create_services_informer(tx.clone());
    let informer_handler = tokio::spawn(async move { svc_informer.run().await });

    while let Some(n) = rx.recv().await {
        if let Err(e) = handle_notification(&mut ipt, n).await {
            tracing::warn!("Error handling notification, caused by: {}", e);
        }
    }

    informer_handler.await??;

    Ok(())
}

async fn handle_notification(ipt: &mut K8sIpTables, n: Notification) -> Result<()> {
    match n {
        Notification::Add(new) => {
            ipt.add_svc(&new);
        },
        Notification::Update(old, new) => {
            // TODO: all right, I am lazy
            ipt.del_svc(&old.name());
            ipt.add_svc(&new);
        },
        Notification::Delete(old) => {
            ipt.del_svc(&old.name());
        },
    }

    Ok(())
}
