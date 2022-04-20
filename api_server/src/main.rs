use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use axum::{routing::post, Extension, Router};
use config::Config;
use etcd::EtcdConfig;
use resources::objects::KubeObject;
use serde::Deserialize;

mod etcd;
mod handler;

#[derive(Debug, Deserialize)]
struct ServerConfig {
    log_level: String,
    etcd: EtcdConfig,
}

pub struct AppState {
    etcd_pool: etcd::EtcdPool,
    schedule_queue: RwLock<Vec<KubeObject>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // read config
    let config = Config::builder()
        .add_source(config::File::with_name("./examples/api-server/config.yaml"))
        .build()?
        .try_deserialize::<ServerConfig>()
        .with_context(|| "Failed to parse config".to_string())?;

    // init tracing
    std::env::set_var("RUST_LOG", format!("api_server={}", config.log_level));
    tracing_subscriber::fmt::init();

    // init app state
    let app_state = AppState::from_config(&config).await?;
    let shared_state = Arc::new(app_state);

    let app = Router::new()
        .route("/api/v1/pods/:name", post(handler::pod::apply))
        .layer(Extension(shared_state));

    tracing::info!("Listening at 0.0.0.0:8080");
    axum::Server::bind(&"0.0.0.0:8080".parse().unwrap())
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
            .with_context(|| "Failed to create etcd client pool".to_string())?;

        Ok(AppState {
            etcd_pool: pool,
            schedule_queue: RwLock::new(vec![]),
        })
    }
}

async fn shutdown() {
    tokio::signal::ctrl_c()
        .await
        .expect("expect tokio signal ctrl-c");
    tracing::info!("Shutting Down");
}
