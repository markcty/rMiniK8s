#[macro_use]
extern crate lazy_static;

use std::env;

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Client, Request, Response, Server,
};
use reqwest::Url;
use resources::models::NodeConfig;
use tokio::sync::mpsc::{self};

use crate::utils::create_func_informer;

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

#[derive(Debug, Clone)]
pub struct Notification;

mod utils;

async fn activate(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let func_name = req
        .headers()
        .get("FUNCTION-NAME")
        .expect("Header should have FUNCTION-NAME")
        .to_str()
        .unwrap()
        .to_owned();
    tracing::info!("Activating function {}", func_name);

    // TODO: create hpa

    // wait until function is ready
    let (tx, mut rx) = mpsc::channel::<Notification>(16);
    let func_informer = create_func_informer(tx, func_name.to_owned());
    let func_store = func_informer.get_store();
    let func_informer_handler = tokio::spawn(async move { func_informer.run().await });

    while rx.recv().await.is_some() {
        let func_store = func_store.read().await;
        if let Some(func) = func_store.get(format!("/api/v1/functions/{}", func_name).as_str()) {
            if func.status.ready {
                break;
            }
        }
        tracing::info!("Function {} not ready", func_name);
    }
    func_informer_handler.abort();
    tracing::info!("Function {} activated", func_name);

    let client = Client::new();
    let res = client.request(req).await?;
    tracing::info!("Return hanged request of function {}", func_name);
    Ok(res)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Function ingress controller started");

    let addr = ([0, 0, 0, 0], 8180).into();
    let service = make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(activate)) });
    let server = Server::bind(&addr).serve(service);
    let graceful = server.with_graceful_shutdown(async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C signal handler");
    });

    println!("Listening on http://{}", addr);

    graceful.await?;

    Ok(())
}
