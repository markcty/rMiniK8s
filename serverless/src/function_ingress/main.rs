#[macro_use]
extern crate lazy_static;

use std::{env, net::Ipv4Addr};

use crate::{
    nginx_config::{IngressHost, NginxConfig},
    utils::update_function,
};

mod nginx_config;
mod utils;

use anyhow::Result;
use reqwest::Url;
use resources::{
    informer::Store,
    models::NodeConfig,
    objects::{function::Function, service::Service},
};
use tokio::sync::mpsc;

use crate::utils::{create_func_informer, create_svc_informer};

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
    tracing::info!("Function ingress controller started");

    let (tx, mut rx) = mpsc::channel::<Notification>(16);

    let svc_informer = create_svc_informer(tx.clone());
    let svc_store = svc_informer.get_store();
    let ingress_informer = create_func_informer(tx.clone());
    let func_store = ingress_informer.get_store();

    let pod_informer_handler = tokio::spawn(async move { svc_informer.run().await });
    let svc_informer_handler = tokio::spawn(async move { ingress_informer.run().await });

    while rx.recv().await.is_some() {
        if let Err(e) = reconfigure_nginx(func_store.to_owned(), svc_store.to_owned()).await {
            tracing::warn!("Error handling notification, caused by: {}", e);
        }
    }

    pod_informer_handler.await??;
    svc_informer_handler.await??;

    Ok(())
}

async fn reconfigure_nginx(func_store: Store<Function>, svc_store: Store<Service>) -> Result<()> {
    let mut func_store = func_store.write().await;
    let svc_store = svc_store.read().await;

    let mut config = NginxConfig::new();
    for (_, func) in func_store.iter_mut() {
        let svc = if let Some(svc) = svc_store.get(&func.spec.service_ref) {
            svc
        } else {
            tracing::warn!(
                "Service of function {} does not exist",
                func.spec.service_ref
            );
            continue;
        };

        let mut host = IngressHost::new(&func.spec.host, func.metadata.name.as_str());

        // if the service is not available, route to activator
        if svc.spec.endpoints.is_empty() {
            // TODO: replace the ip with the real activator ip
            host.add_path("/", &Ipv4Addr::new(127, 0, 0, 1), &8180);
            tracing::info!(
                "Funcion {}'s Service is not ready, route to activator",
                func.metadata.name
            );
        } else {
            host.add_path("/", svc.spec.cluster_ip.as_ref().unwrap(), &80);
            tracing::info!(
                "Funcion {}'s Service is ready, route to cluster_ip {}",
                func.metadata.name,
                svc.spec.cluster_ip.as_ref().unwrap()
            );
            if !func.status.ready {
                func.status.ready = true;
                update_function(func).await?;
            }
        }
        config.add_host(host);
    }
    config.flush();

    Ok(())
}
