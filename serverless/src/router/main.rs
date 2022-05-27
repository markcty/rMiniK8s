#[macro_use]
extern crate lazy_static;

use std::env;

use anyhow::Result;
use hyper::{
    service::{make_service_fn, service_fn},
    Server,
};
use prometheus::{opts, register_counter_vec, CounterVec};
use reqwest::Url;
use resources::{
    models::NodeConfig,
    objects::{function::Function, service::Service},
};

use crate::{route::router, utils::create_informer};

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
    static ref REQUESTS_COUNTER: CounterVec = register_counter_vec!(
        opts!("function_requests_total", "Total number of requests"),
        &["function"] // labels
    )
    .unwrap();
}

mod route;
mod utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Serverless router started");

    // let (tx, mut rx) = mpsc::channel::<Notification>(16);
    let func_informer = create_informer::<Function>("functions");
    let func_store = func_informer.get_store();
    let func_informer_handler = tokio::spawn(async move { func_informer.run().await });

    let svc_informer = create_informer::<Service>("services");
    let svc_store = svc_informer.get_store();
    let svc_informer_handler = tokio::spawn(async move { svc_informer.run().await });

    let addr = ([0, 0, 0, 0], 80).into();
    let service = make_service_fn(move |_| {
        let func_store = func_store.clone();
        let svc_store = svc_store.clone();

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                let func_store = func_store.clone();
                let svc_store = svc_store.clone();
                async move { Ok::<_, hyper::Error>(router(req, func_store, svc_store).await) }
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
    svc_informer_handler.abort();

    Ok(())
}
