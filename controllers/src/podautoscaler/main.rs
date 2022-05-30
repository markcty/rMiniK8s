#[macro_use]
extern crate lazy_static;

use anyhow::{Context, Result};
use config::{Config, File};
use resources::config::ClusterConfig;

mod horizontal;
mod metrics;
mod replica_calculator;
mod utils;

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

/// Default sync period in seconds
pub static SYNC_PERIOD: u32 = 15;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let mut controller = horizontal::PodAutoscaler::new();
    controller.run().await?;
    Ok(())
}
