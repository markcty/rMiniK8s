use anyhow::{anyhow, Result};
use hyper::{http::uri::Scheme, Body, Client, Request, Response, StatusCode, Uri};
use resources::{
    informer::Store,
    objects::{function::Function, service::Service, KubeObject},
};

use crate::CONFIG;

async fn route(mut req: Request<Body>, func_store: Store<Function>) -> Result<Response<Body>> {
    let need_activate;
    let func_key;
    // find the function
    {
        let host = req
            .headers()
            .get(hyper::header::HOST)
            .ok_or_else(|| anyhow!("No host"))?
            .to_str()?;

        let func_store = func_store.read().await;
        let (key, func) = func_store
            .iter()
            .find(|(_, func)| func.spec.host == host)
            .ok_or_else(|| anyhow!("No such function"))?;
        func_key = key.to_owned();
        need_activate = !func.status.ready;
    }

    if need_activate {
        activate(func_key.clone(), func_store.clone()).await?;
    }

    // push metrics
    metric(func_key.as_str());

    // get real service
    let svc_name = func_store
        .read()
        .await
        .get(&func_key)
        .ok_or_else(|| anyhow!("No such function"))?
        .spec
        .service_ref
        .clone();
    let svc = get_svc(svc_name.as_str()).await?;

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
}

// TODO
fn metric(_func: &str) {}

async fn get_svc(svc_name: &str) -> Result<Service> {
    let client = reqwest::Client::new();
    let url = CONFIG.api_server_endpoint.to_owned().join(svc_name)?;
    let res = client
        .get(url)
        .send()
        .await?
        .json::<resources::models::Response<KubeObject>>()
        .await?;
    if let KubeObject::Service(svc) = res.data.ok_or_else(|| anyhow!("No such service"))? {
        Ok(svc)
    } else {
        Err(anyhow!("No such service"))
    }
}

async fn activate(func_key: String, func_store: Store<Function>) -> Result<()> {
    // TODO: create hpa or ...

    let mut i = 0;
    loop {
        {
            let func_store = func_store.read().await;

            let func = func_store
                .get(&func_key)
                .ok_or_else(|| anyhow!("No such function"))?;
            if func.status.ready {
                break;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        if i % 5 == 0 {
            tracing::info!("Function {} is still pending, {}s", func_key, i);
        }
        if i == 60 {
            tracing::error!("Function {} activation timeout after 60s", func_key);
            return Err(anyhow!("Function activation timeout after 60s"));
        }
        i += 1;
    }
    Ok(())
}

pub async fn router(req: Request<Body>, func_store: Store<Function>) -> Response<Body> {
    match route(req, func_store).await {
        Ok(res) => res,
        Err(err) => {
            let mut res = Response::new(Body::from(err.to_string()));
            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            res
        },
    }
}
