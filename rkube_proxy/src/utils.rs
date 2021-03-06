use anyhow::{anyhow, Ok};
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler, Store},
    models::Response,
    objects::service::Service,
};
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;

use crate::{Notification, ResyncNotification, CONFIG};

pub fn create_services_informer(
    tx: Sender<Notification>,
    tx_resync: Sender<ResyncNotification>,
) -> (Informer<Service>, Store<Service>) {
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get(CONFIG.api_server_endpoint.join("/api/v1/services")?)
                    .await?
                    .json::<Response<Vec<Service>>>()
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
    let rh = ResyncHandler(Box::new(move |_| {
        let resync_tx = tx_resync.clone();
        Box::pin(async move {
            resync_tx.send(ResyncNotification).await?;
            Ok(())
        })
    }));

    // start the informer
    let informer = Informer::new(lw, eh, rh);
    let store = informer.get_store();
    (informer, store)
}
