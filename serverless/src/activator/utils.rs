use anyhow::anyhow;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler},
    models::{self},
    objects::function::Function,
};
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;

use crate::{Notification, CONFIG};

pub fn create_func_informer(tx: Sender<Notification>, name: String) -> Informer<Function> {
    let name2 = name.clone();
    let lw = ListerWatcher {
        lister: Box::new(move |_| {
            let name = name.clone();
            Box::pin(async move {
                let res = reqwest::get(
                    CONFIG
                        .api_server_endpoint
                        .join(format!("/api/v1/functions/{}", name).as_str())?,
                )
                .await?
                .json::<models::Response<Function>>()
                .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                anyhow::Ok(vec![res])
            })
        }),
        watcher: Box::new(move |_| {
            let name = name2.clone();
            Box::pin(async move {
                let mut url = CONFIG
                    .api_server_endpoint
                    .join(format!("/api/v1/watch/functions/{}", name).as_str())?;
                url.set_scheme("ws").ok();
                let (stream, _) = connect_async(url).await?;
                anyhow::Ok(stream)
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
