use anyhow::{anyhow, Error, Result};
use reqwest::Url;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, WsStream},
    models,
    objects::KubeObject,
};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // create list watcher closures
    // TODO: maybe some crate or macros can simplify the tedious boxed closure creation in heap
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get("http://localhost:8080/api/v1/pods")
                    .await?
                    .json::<models::Response<Vec<KubeObject>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<KubeObject>, Error>(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let url = Url::parse("ws://localhost:8080/api/v1/watch/pods")?;
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    };

    // create event handler closures
    let (tx_add, mut rx) = mpsc::channel::<String>(16);
    let tx_update = tx_add.clone();
    let tx_delete = tx_add.clone();
    let eh = EventHandler {
        add_cls: Box::new(move |new| {
            // TODO: this is not good: tx is copied every time add_cls is called, but I can't find a better way
            let tx_add = tx_add.clone();
            Box::pin(async move {
                let message = format!("add\n{:#?}", new);
                tx_add.send(message).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old, new)| {
            let tx_update = tx_update.clone();
            Box::pin(async move {
                let message = format!("update\n{:#?}\n{:#?}", old, new);
                tx_update.send(message).await?;
                Ok(())
            })
        }),
        delete_cls: Box::new(move |old| {
            let tx_delete = tx_delete.clone();
            Box::pin(async move {
                let message = format!("delete\n{:#?}", old);
                tx_delete.send(message).await?;
                Ok(())
            })
        }),
    };

    // start the informer
    let informer = Informer::new(lw, eh);
    let informer_handler = tokio::spawn(async move { informer.run().await });

    while let Some(s) = rx.recv().await {
        // you can do deserializing and controller related stuff here
        println!("{}", s);
    }

    informer_handler.await?
}
