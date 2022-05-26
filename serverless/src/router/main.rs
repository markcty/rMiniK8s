#[macro_use]
extern crate lazy_static;

use std::env;

use anyhow::Result;
use hyper::{
    service::{make_service_fn, service_fn},
    Server,
};
use reqwest::Url;
use resources::models::NodeConfig;

use crate::{route::router, utils::create_func_informer};

lazy_static! {
    static ref CONFIG: NodeConfig = {
        dotenv::from_path("/etc/rminik8s/node.env").ok();
        NodeConfig {
            etcd_endpoint: match env::var("ETCD_ENDPOINT") {
                Ok(url) => Url::parse(url.as_str()).unwrap(),
                Err(_) => Url::parse("http://127.0.0.1:2379/").unwrap(),
            },
            api_server_endpoint: match env::var("API_SERVER_ENDPOINT") {
                Ok(url) => Url::parse(url.as_str()).unwrap(),
                Err(_) => Url::parse("http://127.0.0.1:8080/").unwrap(),
            },
        }
    };
}

mod route;
mod utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Serverless router started");

    // let (tx, mut rx) = mpsc::channel::<Notification>(16);
    let func_informer = create_func_informer();
    let func_store = func_informer.get_store();
    let func_informer_handler = tokio::spawn(async move { func_informer.run().await });

    let addr = ([0, 0, 0, 0], 80).into();
    let service = make_service_fn(move |_| {
        let func_store = func_store.clone();

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                let func_store = func_store.clone();
                async move { Ok::<_, hyper::Error>(router(req, func_store).await) }
            }))
        }
    });

    let server = Server::bind(&addr).serve(service);
    let graceful = server.with_graceful_shutdown(async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C signal handler");
    });

    tracing::info!("Listening on http://{}", addr);

    graceful.await?;
    func_informer_handler.abort();

    Ok(())
}
