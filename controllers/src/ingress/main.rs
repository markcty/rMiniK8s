#[macro_use]
extern crate lazy_static;

use std::env;

use nginx_ingress_config::{IngressHost, NginxIngressConfig};

mod nginx_ingress_config;
mod utils;

use anyhow::Result;
use reqwest::Url;
use resources::{
    informer::Store,
    models::NodeConfig,
    objects::{ingress::Ingress, service::Service},
};
use tokio::sync::mpsc;

use crate::utils::{create_ingress_informer, create_svc_informer};

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

#[derive(Debug)]
pub struct Notification;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("Endpoints controller started");

    let (tx, mut rx) = mpsc::channel::<Notification>(16);

    let svc_informer = create_svc_informer(tx.clone());
    let svc_store = svc_informer.get_store();
    let ingress_informer = create_ingress_informer(tx.clone());
    let ingress_store = ingress_informer.get_store();

    let pod_informer_handler = tokio::spawn(async move { svc_informer.run().await });
    let svc_informer_handler = tokio::spawn(async move { ingress_informer.run().await });

    while rx.recv().await.is_some() {
        if let Err(e) = reconfigure_nginx(ingress_store.to_owned(), svc_store.to_owned()).await {
            tracing::warn!("Error handling notification, caused by: {}", e);
        }
    }

    pod_informer_handler.await??;
    svc_informer_handler.await??;

    Ok(())
}

async fn reconfigure_nginx(ingress_store: Store<Ingress>, svc_store: Store<Service>) -> Result<()> {
    let mut config = NginxIngressConfig::new();
    let ingress_store = ingress_store.read().await;
    let svc_store = svc_store.read().await;

    for (_, ingress) in ingress_store.iter() {
        let ingress_rules = &ingress.spec.rules;

        for rule in ingress_rules.iter() {
            let mut host = IngressHost::new(rule.host.as_ref().unwrap());

            for path in rule.paths.iter() {
                let svc_uri = format!("/api/v1/services/{}", path.service.name);
                if let Some(svc) = svc_store.get(svc_uri.as_str()) {
                    let cluster_ip = svc.spec.cluster_ip.as_ref().unwrap();
                    host.add_path(&path.path, cluster_ip, &path.service.port);
                    tracing::info!(
                        "add path {}{} for service {}:{}:{}",
                        rule.host.as_ref().unwrap(),
                        path.path,
                        path.service.name,
                        cluster_ip,
                        path.service.port
                    );
                } else {
                    tracing::warn!(
                        "Failed to add service {} to path {}, no such service exists",
                        path.service.name,
                        path.path
                    );
                }
            }

            config.add_host(host);
        }
    }

    config.flush();
    Ok(())
}
