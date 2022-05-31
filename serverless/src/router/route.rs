use anyhow::{anyhow, Context, Result};
use hyper::{http::uri::Scheme, Body, Client, Request, Response, StatusCode, Uri};
use prometheus::{Encoder, TextEncoder};
use resources::{
    informer::Store,
    objects::{function::Function, service::Service, KubeObject, Object},
};

use crate::{workflow::handle_workflow, CONFIG, REQUESTS_COUNTER};

async fn route(
    mut req: Request<Body>,
    func_store: Store<Function>,
    svc_store: Store<Service>,
) -> Result<Response<Body>> {
    let need_activate;
    let func_key;
    let func_name;
    let svc_key;
    let host = req
        .headers()
        .get(hyper::header::HOST)
        .ok_or_else(|| anyhow!("No host"))?
        .to_str()?
        // strip off the port
        .split(':')
        .next()
        .ok_or_else(|| anyhow!("No host"))?
        .to_owned();

    if host.ends_with(".workflow.func.minik8s.com") {
        // find the workflow
        let workflow = host
            .split('.')
            .next()
            .ok_or_else(|| anyhow!("No such workflow"))?;
        let body = String::from_utf8(hyper::body::to_bytes(req.body_mut()).await?.to_vec())?;
        handle_workflow(workflow, body).await
    } else if host.ends_with(".func.minik8s.com") {
        {
            // find the function
            let func_store = func_store.read().await;
            let (key, func) = func_store
                .iter()
                .find(|(_, func)| func.status.as_ref().unwrap().host == host)
                .ok_or_else(|| anyhow!("No such function matching host: {}", host))?;
            func_key = key.to_owned();
            func_name = func.metadata.name.to_owned();

            svc_key = func.status.as_ref().unwrap().service_ref.clone();
            let svc = get_svc(&svc_key, svc_store.to_owned()).await?;
            need_activate = svc.spec.endpoints.is_empty();
        }

        REQUESTS_COUNTER.with_label_values(&[&func_name]).inc();
        if need_activate {
            activate(&func_name, svc_key.clone(), svc_store.clone()).await?;
        }

        let svc = get_svc(&svc_key, svc_store).await?;

        // build new uri
        let uri = req.uri_mut();
        tracing::info!("{}", uri);
        let new_uri = Uri::builder()
            .scheme(Scheme::HTTP)
            .authority(svc.spec.cluster_ip.unwrap().to_string())
            .path_and_query(uri.path_and_query().unwrap().as_str())
            .build()?;
        *uri = new_uri;

        tracing::info!("Forward {} to {}", func_key, uri);

        // forward to real service
        let client = Client::new();
        Ok(client.request(req).await?)
    } else {
        tracing::warn!("Incorrect host: {}", host);
        let mut res = Response::new(Body::from("No such host"));
        *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        Ok(res)
    }
}

/// Return Prometheus metrics
fn metrics() -> Result<String> {
    let metrics = prometheus::gather();
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    encoder.encode(&metrics, &mut buffer)?;
    String::from_utf8(buffer).with_context(|| "Failed to encode metrics")
}

async fn get_svc(svc_key: &str, svc_store: Store<Service>) -> Result<Service> {
    let svc_store = svc_store.read().await;
    let (_, svc) = svc_store
        .iter()
        .find(|(key, _)| svc_key.eq(*key))
        .ok_or_else(|| anyhow!("No such service: {}", svc_key))?;
    Ok(svc.to_owned())
}

async fn activate_rs(rs_name: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}api/v1/replicasets/{}",
        CONFIG.api_server_endpoint, rs_name
    );
    let res = client
        .get(url)
        .send()
        .await?
        .json::<resources::models::Response<KubeObject>>()
        .await?;
    let mut object = res
        .data
        .ok_or_else(|| anyhow!("No such replicaset: {}", rs_name))?;
    if let KubeObject::ReplicaSet(ref mut rs) = object {
        // Scale ReplicaSet from 0 to 1
        rs.spec.replicas = 1;
        let url = CONFIG.api_server_endpoint.to_owned().join(&object.uri())?;
        let res = client
            .patch(url)
            .json(&object)
            .send()
            .await?
            .json::<resources::models::Response<String>>()
            .await?;
        tracing::info!("{}", res.msg.unwrap_or_default());
        Ok(())
    } else {
        Err(anyhow!("No such replicaset: {}", rs_name))
    }
}

async fn activate(func_name: &str, svc_key: String, svc_store: Store<Service>) -> Result<()> {
    tracing::info!("Function {} has no instance, activating...", func_name);
    // TODO: create hpa or ...
    activate_rs(func_name).await?;

    let mut i = 0;
    loop {
        {
            let svc = get_svc(&svc_key, svc_store.clone()).await?;
            if !svc.spec.endpoints.is_empty() {
                break;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        if i % 5 == 0 {
            tracing::info!("Function {} is still pending, {}s", func_name, i);
        }
        if i == 60 {
            tracing::error!("Function {} activation timeout after 60s", func_name);
            return Err(anyhow!("Function activation timeout after 60s"));
        }
        i += 1;
    }
    Ok(())
}

pub async fn router(
    req: Request<Body>,
    func_store: Store<Function>,
    svc_store: Store<Service>,
) -> Response<Body> {
    if req.uri().path() == "/metrics" {
        match metrics() {
            Ok(res) => {
                tracing::debug!("Get metrics succeeded");
                Response::new(Body::from(res))
            },
            Err(err) => {
                tracing::error!("Error processing request: {:#}", err);
                let mut res = Response::new(Body::from(err.to_string()));
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                res
            },
        }
    } else {
        match route(req, func_store, svc_store).await {
            Ok(res) => res,
            Err(err) => {
                tracing::error!("Error processing request: {:#}", err);
                let mut res = Response::new(Body::from(err.to_string()));
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                res
            },
        }
    }
}
