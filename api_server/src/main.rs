use std::sync::Arc;

use axum::{routing::get, Extension, Router};
use config::Config;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

mod etcd;

#[derive(Debug, Serialize, Deserialize)]
struct ServerConfig {
    etcd_url: String,
}

// load config
lazy_static! {
    static ref SERVER_CONFIG: ServerConfig = {
        let settings = Config::builder()
            .add_source(config::File::with_name("./examples/api-server/config.yaml"))
            .build()
            .unwrap();
        settings.try_deserialize::<ServerConfig>().unwrap()
    };
}

struct State {
    etcd_client: etcd_client::Client,
}

#[tokio::main]
async fn main() {
    let shared_state = Arc::new(Mutex::new(init_state().await));

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .layer(Extension(shared_state));

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown())
        .await
        .unwrap();
}

async fn init_state() -> State {
    let etcd_client = etcd::connect_etcd().await.unwrap();
    State { etcd_client }
}

async fn shutdown() {
    tokio::signal::ctrl_c()
        .await
        .expect("expect tokio signal ctrl-c");
    println!("\nShutting down...")
}
