use anyhow::{anyhow, Error};
use reqwest::Url;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler, Store, WsStream},
    models,
    objects::{node::Node, pod::Pod, Object},
};
use tokio::{
    sync::mpsc::{self, Receiver},
    task::JoinHandle,
};
use tokio_tungstenite::connect_async;

pub type RunInformerResult<T> = (Receiver<T>, Store<T>, JoinHandle<Result<(), Error>>);

pub fn run_pod_informer() -> RunInformerResult<Pod> {
    // create list watcher closures
    // TODO: maybe some crate or macros can simplify the tedious boxed closure creation in heap
    let lw = ListerWatcher::<Pod> {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get("http://localhost:8080/api/v1/pods")
                    .await?
                    .json::<models::Response<Vec<Pod>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<Pod>, Error>(res)
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
    let (tx_add, rx) = mpsc::channel::<Pod>(16);
    let eh = EventHandler::<Pod> {
        add_cls: Box::new(move |pod| {
            // TODO: this is not good: tx is copied every time add_cls is called, but I can't find a better way
            let tx_add = tx_add.clone();
            Box::pin(async move {
                tracing::debug!("add\n{}", pod.name());
                tx_add.send(pod).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old, new)| {
            Box::pin(async move {
                tracing::debug!("update\n{}\n{}", old.name(), new.name());
                Ok(())
            })
        }),
        delete_cls: Box::new(move |old| {
            Box::pin(async move {
                tracing::debug!("delete\n{}", old.name());
                Ok(())
            })
        }),
    };

    // TODO: fill real logic
    let rh = ResyncHandler(Box::new(move |_| {
        Box::pin(async move {
            tracing::debug!("Resync\n");
            Ok(())
        })
    }));
    // start the informer
    let informer = Informer::new(lw, eh, rh);
    let store = informer.get_store();
    let informer_handler = tokio::spawn(async move { informer.run().await });

    (rx, store, informer_handler)
}

pub fn run_node_informer() -> RunInformerResult<Node> {
    // create list watcher closures
    // TODO: maybe some crate or macros can simplify the tedious boxed closure creation in heap
    let lw = ListerWatcher::<Node> {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get("http://localhost:8080/api/v1/nodes")
                    .await?
                    .json::<models::Response<Vec<Node>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<Node>, Error>(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let url = Url::parse("ws://localhost:8080/api/v1/watch/nodes")?;
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    };

    // create event handler closures
    let (_, rx) = mpsc::channel::<Node>(16);
    let eh = EventHandler::<Node> {
        add_cls: Box::new(move |node| {
            // TODO: this is not good: tx is copied every time add_cls is called, but I can't find a better way
            Box::pin(async move {
                tracing::debug!("add\n{}", node.name());
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old, new)| {
            Box::pin(async move {
                tracing::debug!("update\n{}\n{}", old.name(), new.name());
                Ok(())
            })
        }),
        delete_cls: Box::new(move |old| {
            Box::pin(async move {
                tracing::debug!("delete\n{}", old.name());
                Ok(())
            })
        }),
    };

    // TODO: fill real logic
    let rh = ResyncHandler(Box::new(move |_| {
        Box::pin(async move {
            tracing::debug!("Resync\n");
            Ok(())
        })
    }));
    // start the informer
    let informer = Informer::new(lw, eh, rh);
    let store = informer.get_store();
    let informer_handler = tokio::spawn(async move { informer.run().await });

    (rx, store, informer_handler)
}
