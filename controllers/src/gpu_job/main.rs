#[macro_use]
extern crate lazy_static;

use anyhow::{Context, Result};
use config::{Config, Environment, File};
use controller::GpuJobController;
use resources::config::ClusterConfig;
use serde::{Deserialize, Serialize};

mod controller;
mod utils;

const TMP_DIR: &str = "/tmp/minik8s/job";
const DOCKER_REGISTRY: &str = "minik8s.xyz";
const BASE_IMG: &str = "minik8s.xyz/gpu_server:latest";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    pub username: String,
    pub password: String,
}

lazy_static! {
    pub static ref CONFIG: ClusterConfig = Config::builder()
        .add_source(File::with_name("/etc/rminik8s/controller-manager.yaml").required(false))
        .set_override_option("apiServerUrl", std::env::var("API_SERVER_URL").ok())
        .unwrap()
        .set_override_option(
            "apiServerWatchUrl",
            std::env::var("API_SERVER_WATCH_URL").ok(),
        )
        .unwrap()
        .build()
        .unwrap_or_default()
        .try_deserialize::<ClusterConfig>()
        .with_context(|| "Failed to parse config".to_string())
        .unwrap_or_default();
    pub static ref GPU_SERVER_CONFIG: ServerConfig = Config::builder()
        .add_source(File::with_name("/etc/rminik8s/gpuserver-config.yaml").required(false))
        .add_source(Environment::default())
        .build()
        .unwrap_or_default()
        .try_deserialize::<ServerConfig>()
        .with_context(|| "Failed to parse config".to_string())
        .unwrap();
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let mut controller = GpuJobController::new();
    controller.run().await?;
    Ok(())
}
