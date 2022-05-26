use std::{
    io::{BufRead, BufReader},
    process::ChildStdout,
};

use anyhow::{anyhow, Error};
use reqwest::Url;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler, WsStream},
    models::Response,
    objects::Object,
};
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;

use crate::CONFIG;

#[derive(Debug)]
pub enum Event<T> {
    Add(T),
    Update(T, T),
    Delete(T),
}

#[derive(Debug)]
pub struct ResyncNotification;

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

pub fn create_informer<T: Object>(
    path: String,
    tx: Sender<Event<T>>,
    resync_tx: Sender<ResyncNotification>,
) -> Informer<T> {
    let lw = create_lister_watcher(path);

    let tx_add = tx;
    let tx_update = tx_add.clone();
    let tx_delete = tx_add.clone();
    let eh = EventHandler::<T> {
        add_cls: Box::new(move |new| {
            // TODO: this is not good: tx is copied every time add_cls is called, but I can't find a better way
            let tx_add = tx_add.clone();
            Box::pin(async move {
                tx_add.send(Event::<T>::Add(new)).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old, new)| {
            let tx_update = tx_update.clone();
            Box::pin(async move {
                tx_update.send(Event::<T>::Update(old, new)).await?;
                Ok(())
            })
        }),
        delete_cls: Box::new(move |old| {
            let tx_delete = tx_delete.clone();
            Box::pin(async move {
                tx_delete.send(Event::<T>::Delete(old)).await?;
                Ok(())
            })
        }),
    };
    let rh = ResyncHandler(Box::new(move |()| {
        let resync_tx = resync_tx.clone();
        Box::pin(async move {
            resync_tx.send(ResyncNotification).await?;
            Ok(())
        })
    }));

    Informer::new(lw, eh, rh)
}

pub fn log_command(stdout: ChildStdout) {
    let reader = BufReader::new(stdout);

    reader
        .lines()
        .filter_map(|line| line.ok())
        .for_each(|line| tracing::info!("{}", line));
}
