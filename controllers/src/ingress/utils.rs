use anyhow::anyhow;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler},
    models,
    objects::{ingress::Ingress, service::Service},
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

pub fn create_ingress_informer(tx: Sender<Notification>) -> Informer<Ingress> {
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get(CONFIG.api_server_endpoint.join("/api/v1/ingresses")?)
                    .await?
                    .json::<models::Response<Vec<Ingress>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let mut url = CONFIG.api_server_endpoint.join("/api/v1/watch/ingresses")?;
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
