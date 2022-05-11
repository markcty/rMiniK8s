use std::sync::Arc;

use anyhow::{anyhow, Ok};
use dashmap::DashMap;
use reqwest::Url;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher},
    models::Response,
    objects::KubeObject,
};
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;

use crate::Notification;

pub fn create_services_informer(
    tx: Sender<Notification>,
) -> (Informer<KubeObject>, Arc<DashMap<String, KubeObject>>) {
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get("http://localhost:8080/api/v1/services")
                    .await?
                    .json::<Response<Vec<KubeObject>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let url = Url::parse("ws://localhost:8080/api/v1/watch/services")?;
                let (stream, _) = connect_async(url).await?;
                Ok(stream)
            })
        }),
    };

    // create event handler closures
    let tx_add = tx.clone();
    let tx_update = tx.clone();
    let tx_delete = tx;
    let eh = EventHandler {
        add_cls: Box::new(move |new| {
            let tx_add = tx_add.clone();
            Box::pin(async move {
                tx_add.send(Notification::Add(new)).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old, new)| {
            let tx_update = tx_update.clone();
            Box::pin(async move {
                tx_update.send(Notification::Update(old, new)).await?;
                Ok(())
            })
        }),
        delete_cls: Box::new(move |old| {
            let tx_delete = tx_delete.clone();
            Box::pin(async move {
                tx_delete.send(Notification::Delete(old)).await?;
                Ok(())
            })
        }),
    };

    // start the informer
    let store = Arc::new(DashMap::new());
    (Informer::new(lw, eh, store.clone()), store)
}
