use anyhow::Result;

use crate::{cache::Cache, informer::*, scheduler::Scheduler};

mod algorithm;
mod cache;
mod informer;
mod scheduler;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let (pod_rx, pod_store, pod_informer_handler) = run_pod_informer();
    let (_, node_store, node_informer_handler) = run_node_informer();

    let cache = Cache::new(pod_store.clone(), node_store.clone());
    let sched = Scheduler::new(algorithm::dummy::dummy, cache);
    let scheduler_handle = tokio::spawn(async move { sched.run(pod_rx).await });

    tracing::info!("scheduler started");

    scheduler_handle.await?.expect("scheduler failed.");
    pod_informer_handler.await?.expect("pod informer failed.");
    node_informer_handler.await?
    // TODO: Gracefully shutdown
}
