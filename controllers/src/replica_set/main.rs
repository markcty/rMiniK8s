use anyhow::Result;
use resources::objects::KubeObject;
use tokio::sync::mpsc;

mod controller;
mod utils;

#[actix_rt::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("ReplicaSet Controller started");

    Ok(())
}
