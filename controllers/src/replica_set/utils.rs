use anyhow::{anyhow, Error};
use reqwest::Url;
use resources::{
    informer::{ListerWatcher, WsStream},
    models::Response,
    objects::KubeObject,
};
use tokio_tungstenite::connect_async;

const API_SERVER_URL: &str = "http://localhost:8080";
const API_SERVER_WATCH_URL: &str = "ws://localhost:8080";

pub fn create_lister(path: &'static str) -> ListerWatcher<KubeObject> {
    let list_url = format!("{}/api/v1/{}", API_SERVER_URL, path);
    let watch_url = format!("{}/api/v1/watch/{}", API_SERVER_WATCH_URL, path);
    ListerWatcher {
        lister: Box::new(move |_| {
            let list_url = list_url.clone();
            Box::pin(async {
                let res = reqwest::get(list_url)
                    .await?
                    .json::<Response<Vec<KubeObject>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<KubeObject>, Error>(res)
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
