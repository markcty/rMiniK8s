#[macro_use]
extern crate lazy_static;

use anyhow::{Context, Result};
use config::{Config, File};
use controller::FunctionController;
use resources::config::ClusterConfig;

mod controller;
mod utils;

const TMP_DIR: &str = "/tmp/minik8s/function";
const TEMPLATES_DIR: &str = "/templates/function_wrapper";
const DOCKER_REGISTRY: &str = "minik8s.xyz";

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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    println!("api_server_url: {}", CONFIG.api_server_url);

    let mut controller = FunctionController::new();
    controller.run().await?;
    Ok(())
}
