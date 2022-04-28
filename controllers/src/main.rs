use anyhow::{anyhow, Error, Result};
use reqwest::Url;
use resources::{
    controller::{ListerWatcher, WsStream},
    models,
    objects::KubeObject,
};
use tokio_tungstenite::connect_async;

#[tokio::main]
async fn main() -> Result<()> {
    // TODO: maybe some closure can simplify the tedious closure definition
    let lw = ListerWatcher {
        lister: Box::new(|| {
            Box::pin(async {
                let res = reqwest::get("http://localhost:8080/api/v1/pods")
                    .await?
                    .json::<models::Response<Vec<KubeObject>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<KubeObject>, Error>(res)
            })
        }),
        watcher: Box::new(|| {
            Box::pin(async {
                let url = Url::parse("ws://localhost:8080/api/v1/watch/pods")?;
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    };

    let informer = Informer::new(lw);

    informer.run().await?;

    Ok(())
}
