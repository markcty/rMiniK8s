#[macro_use]
extern crate lazy_static;

use anyhow::{Context, Result};
use config::{Config, File};
use controller::ReplicaSetController;
use resources::config::ClusterConfig;

mod controller;
mod utils;

lazy_static! {
    pub static ref CONFIG: ClusterConfig = Config::builder()
        .add_source(File::with_name("/etc/rminik8s/controller-manager.yaml"))
        .build()
        .unwrap_or_default()
        .try_deserialize::<ClusterConfig>()
        .with_context(|| "Failed to parse config".to_string())
        .unwrap_or_default();
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let mut controller = ReplicaSetController::new();
    controller.run().await?;
    Ok(())
}
