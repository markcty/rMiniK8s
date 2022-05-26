use anyhow::anyhow;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler},
    models::{self},
    objects::function::Function,
};
use tokio_tungstenite::connect_async;

use crate::CONFIG;

pub fn create_func_informer() -> Informer<Function> {
    let lw = ListerWatcher {
        lister: Box::new(move |_| {
            Box::pin(async move {
                let res = reqwest::get(CONFIG.api_server_endpoint.join("/api/v1/functions")?)
                    .await?
                    .json::<models::Response<Vec<Function>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                anyhow::Ok(res)
            })
        }),
        watcher: Box::new(move |_| {
            Box::pin(async move {
                let mut url = CONFIG.api_server_endpoint.join("/api/v1/watch/functions")?;
                url.set_scheme("ws").ok();
                let (stream, _) = connect_async(url).await?;
                anyhow::Ok(stream)
            })
        }),
    };

    // create event handler closures
    let eh = EventHandler {
        add_cls: Box::new(move |_| Box::pin(async move { Ok(()) })),
        update_cls: Box::new(move |_| Box::pin(async move { Ok(()) })),
        delete_cls: Box::new(move |_| Box::pin(async move { Ok(()) })),
    };

    let rh = ResyncHandler(Box::new(move |_| Box::pin(async move { Ok(()) })));

    // start the informer
    Informer::new(lw, eh, rh)
}
