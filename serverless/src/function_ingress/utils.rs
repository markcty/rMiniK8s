use anyhow::{anyhow, Result};
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler},
    models::{self, ErrResponse},
    objects::{function::Function, service::Service, KubeObject, Object},
};
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;

use crate::{Notification, CONFIG};

pub fn create_svc_informer(tx: Sender<Notification>) -> Informer<Service> {
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get(CONFIG.api_server_endpoint.join("/api/v1/services")?)
                    .await?
                    .json::<models::Response<Vec<Service>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let mut url = CONFIG.api_server_endpoint.join("/api/v1/watch/services")?;
                url.set_scheme("ws").ok();
                let (stream, _) = connect_async(url).await?;
                Ok(stream)
            })
        }),
    };

    // create event handler closures
    let tx_add = tx.clone();
    let tx_update = tx.clone();
    let tx_delete = tx.clone();
    let eh = EventHandler {
        add_cls: Box::new(move |_| {
            let tx_add = tx_add.clone();
            Box::pin(async move {
                tx_add.send(Notification).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |_| {
            let tx_update = tx_update.clone();
            Box::pin(async move {
                tx_update.send(Notification).await?;
                Ok(())
            })
        }),
        delete_cls: Box::new(move |_| {
            let tx_delete = tx_delete.clone();
            Box::pin(async move {
                tx_delete.send(Notification).await?;
                Ok(())
            })
        }),
    };

    let rh = ResyncHandler(Box::new(move |_| {
        let tx = tx.clone();
        Box::pin(async move {
            tx.send(Notification).await?;
            Ok(())
        })
    }));

    // start the informer
    Informer::new(lw, eh, rh)
}

pub fn create_func_informer(tx: Sender<Notification>) -> Informer<Function> {
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get(CONFIG.api_server_endpoint.join("/api/v1/functions")?)
                    .await?
                    .json::<models::Response<Vec<Function>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let mut url = CONFIG.api_server_endpoint.join("/api/v1/watch/functions")?;
                url.set_scheme("ws").ok();
                let (stream, _) = connect_async(url).await?;
                Ok(stream)
            })
        }),
    };

    // create event handler closures
    let tx_add = tx.clone();
    let tx_update = tx.clone();
    let tx_delete = tx.clone();
    let eh = EventHandler {
        add_cls: Box::new(move |_| {
            let tx_add = tx_add.clone();
            Box::pin(async move {
                tx_add.send(Notification).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |_| {
            let tx_update = tx_update.clone();
            Box::pin(async move {
                tx_update.send(Notification).await?;
                Ok(())
            })
        }),
        delete_cls: Box::new(move |_| {
            let tx_delete = tx_delete.clone();
            Box::pin(async move {
                tx_delete.send(Notification).await?;
                Ok(())
            })
        }),
    };

    let rh = ResyncHandler(Box::new(move |_| {
        let tx = tx.clone();
        Box::pin(async move {
            tx.send(Notification).await?;
            Ok(())
        })
    }));

    // start the informer
    Informer::new(lw, eh, rh)
}

pub async fn update_function(func: &Function) -> Result<()> {
    let client = reqwest::Client::new();
    let res = client
        .put(CONFIG.api_server_endpoint.join(func.uri().as_str())?)
        .json(&KubeObject::Function(func.to_owned()))
        .send()
        .await?;
    if res.error_for_status_ref().is_err() {
        let res = res.json::<ErrResponse>().await?;
        tracing::error!("Error update function: {}", res.msg);
    }
    Ok(())
}
