#[macro_use]
extern crate lazy_static;

use std::env;

use anyhow::Result;
use reqwest::Url;
use resources::{
    models::NodeConfig,
    objects::{node::Node, pod::Pod},
};
use tokio::sync::mpsc;

use crate::{cache::Cache, informer::*, scheduler::Scheduler};

mod algorithm;
mod cache;
mod informer;
mod scheduler;

#[derive(Debug)]
pub enum PodUpdate {
    Add(Pod),
    Update(Pod, Pod),
    Delete(Pod),
}

#[derive(Debug)]
pub enum NodeUpdate {
    Add(Node),
    Update(Node, Node),
    Delete(Node),
}

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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let (resync_tx, resync_rx) = mpsc::channel::<ResyncNotification>(16);

    let (pod_tx, pod_rx, pod_store, pod_informer_handler) = run_pod_informer(resync_tx.clone());
    let (_, node_rx, node_store, node_informer_handler) = run_node_informer(resync_tx);

    let cache = Cache::new(pod_store.clone(), node_store.clone());
    let mut sched = Scheduler::new(algorithm::simple::simple, cache, resync_rx, pod_tx);
    let scheduler_handle = tokio::spawn(async move { sched.run(pod_rx, node_rx).await });

    scheduler_handle.await?;
    pod_informer_handler.await??;
    node_informer_handler.await?
    // TODO: Gracefully shutdown
}
