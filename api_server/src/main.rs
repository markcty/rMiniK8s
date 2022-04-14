use anyhow::{Context, Result};
use axum::{routing::get, Extension, Router};
use config::Config;
use serde::Deserialize;
use tracing;

use etcd::EtcdConfig;

mod etcd;

#[derive(Debug, Deserialize)]
struct ServerConfig {
    log_level: String,
    etcd: EtcdConfig,
}

#[derive(Clone)]
struct AppState {
    etcd_pool: etcd::EtcdPool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // read config
    let config = Config::builder()
        .add_source(config::File::with_name("./examples/api-server/config.yaml"))
        .build()?
        .try_deserialize::<ServerConfig>()
        .with_context(|| format!("Failed to parse config"))?;

    // init tracing
    std::env::set_var("RUST_LOG", format!("api_server={}", config.log_level));
    tracing_subscriber::fmt::init();

    // init app state
    let app_state = AppState::from_config(&config).await?;

    let app = Router::new()
        .route("/", get(|| async { "Hello from api_server!" }))
        .layer(Extension(app_state));

    tracing::info!("Listening at 0.0.0.0:3000");
    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown())
        .await
        .unwrap();

    Ok(())
}

impl AppState {
    async fn from_config(config: &ServerConfig) -> Result<AppState> {
        let pool = config
            .etcd
            .create_pool()
            .await
            .with_context(|| format!("Failed to create etcd client pool"))?;

        Ok(AppState { etcd_pool: pool })
    }
}

async fn shutdown() {
    tokio::signal::ctrl_c()
        .await
        .expect("expect tokio signal ctrl-c");
    tracing::info!("Shutting Down");
}
