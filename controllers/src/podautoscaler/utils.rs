use anyhow::{anyhow, Error, Result};
use reqwest::Url;
use resources::{
    informer::{ListerWatcher, WsStream},
    models::Response,
    objects::{object_reference::ObjectReference, KubeObject, Object},
};
use tokio_tungstenite::connect_async;

use crate::CONFIG;

pub fn create_lister_watcher<T: Object>(path: String) -> ListerWatcher<T> {
    let list_url = format!("{}/api/v1/{}", CONFIG.api_server_url, path);
    let watch_url = format!("{}/api/v1/watch/{}", CONFIG.api_server_watch_url, path);
    ListerWatcher {
        lister: Box::new(move |_| {
            let list_url = list_url.clone();
            Box::pin(async {
                let res = reqwest::get(list_url)
                    .await?
                    .json::<Response<Vec<T>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<T>, Error>(res)
            })
        }),
        watcher: Box::new(move |_| {
            let watch_url = watch_url.clone();
            Box::pin(async move {
                let url = Url::parse(watch_url.as_str())?;
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    }
}

pub async fn get_scale_target(target: &ObjectReference) -> Result<KubeObject> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/api/v1/{}s/{}",
            CONFIG.api_server_url,
            target.kind.to_lowercase(),
            target.name,
        ))
        .send()
        .await?
        .json::<Response<KubeObject>>()
        .await?;
    match response.data {
        Some(data) => Ok(data),
        None => Err(anyhow!("Failed to get scale target")),
    }
}

pub async fn post_update(object: &KubeObject) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .put(format!("{}{}", CONFIG.api_server_url, object.uri()))
        .json(object)
        .send()
        .await?
        .json::<Response<()>>()
        .await?;
    if let Some(msg) = response.msg {
        tracing::info!("{}", msg);
    }
    Ok(())
}
