#[macro_use]
extern crate lazy_static;

use anyhow::{Context, Result};
use config::{Config, File};
use controller::FunctionController;
use resources::config::ClusterConfig;
use serde::{Deserialize, Serialize};

mod controller;
mod utils;

const TMP_DIR: &str = "/tmp/minik8s/function";
const TEMPLATES_DIR: &str = "./templates/function_wrapper";
const DOCKER_REGISTRY: &str = "minik8s.xyz";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    pub username: String,
    pub password: String,
}

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

    let mut controller = FunctionController::new();
    controller.run().await?;
    Ok(())
}
