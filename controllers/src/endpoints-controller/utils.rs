use std::sync::Arc;

use anyhow::{anyhow, Error, Result};
use dashmap::DashMap;
use reqwest::Url;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, WsStream},
    models,
    objects::{
        KubeObject,
        KubeResource::{Pod, Service},
        Object,
    },
    utils::selector_match,
};
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;

use crate::{Notification, PodNtf, ServiceNtf};

pub fn create_services_informer(
    tx: Sender<Notification>,
) -> (Informer<KubeObject>, Arc<DashMap<String, KubeObject>>) {
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get("http://localhost:8080/api/v1/services")
                    .await?
                    .json::<models::Response<Vec<KubeObject>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok::<Vec<KubeObject>, Error>(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let url = Url::parse("ws://localhost:8080/api/v1/watch/services")?;
                let (stream, _) = connect_async(url).await?;
                Ok::<WsStream, Error>(stream)
            })
        }),
    };

    // create event handler closures
    let tx_add = tx;
    let eh = EventHandler {
        add_cls: Box::new(move |new| {
            let tx_add = tx_add.clone();
            Box::pin(async move {
                tx_add
                    .send(Notification::Service(ServiceNtf::Add(new)))
                    .await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |_| Box::pin(async move { Ok(()) })),
        delete_cls: Box::new(move |_| Box::pin(async move { Ok(()) })),
    };

    // start the informer
    let store = Arc::new(DashMap::new());
    (Informer::new(lw, eh, store.clone()), store)
}

pub fn create_pods_informer(
    tx: Sender<Notification>,
) -> (Informer<KubeObject>, Arc<DashMap<String, KubeObject>>) {
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
    let tx_add = tx.clone();
    let tx_update = tx.clone();
    let tx_delete = tx.clone();
    let eh = EventHandler {
        add_cls: Box::new(move |new| {
            let tx_add = tx_add.clone();
            Box::pin(async move {
                tx_add.send(Notification::Pod(PodNtf::Add(new))).await?;
                Ok(())
            })
        }),
        update_cls: Box::new(move |(old, new)| {
            let tx_update = tx_update.clone();
            Box::pin(async move {
                tx_update
                    .send(Notification::Pod(PodNtf::Update(old, new)))
                    .await?;
                Ok(())
            })
        }),
        delete_cls: Box::new(move |old| {
            let tx_delete = tx_delete.clone();
            Box::pin(async move {
                tx_delete
                    .send(Notification::Pod(PodNtf::Delete(old)))
                    .await?;
                Ok(())
            })
        }),
    };

    // start the informer
    let store = Arc::new(DashMap::new());
    (Informer::new(lw, eh, store.clone()), store)
}

pub async fn add_svc_endpoint(
    svc_store: Arc<DashMap<String, KubeObject>>,
    pod: KubeObject,
) -> Result<()> {
    let pod_ip = if let Pod(pod) = pod.resource {
        if let Some(ip) = pod.get_ip() {
            ip
        } else {
            return Ok(());
        }
    } else {
        return Ok(());
    };

    for mut svc_ref in svc_store.iter_mut() {
        let svc = if let Service(ref mut svc) = svc_ref.resource {
            svc
        } else {
            continue;
        };

        if selector_match(&svc.spec.selector, &pod.metadata.labels)
            && !svc.spec.endpoints.contains(&pod_ip)
        {
            svc.spec.endpoints.push(pod_ip);
            update_service(svc_ref.value()).await?;
        }
    }
    Ok(())
}

pub async fn del_svc_endpoint(
    svc_store: Arc<DashMap<String, KubeObject>>,
    pod: KubeObject,
) -> Result<()> {
    let pod_ip = if let Pod(pod) = pod.resource {
        if let Some(ip) = pod.get_ip() {
            ip
        } else {
            return Ok(());
        }
    } else {
        return Ok(());
    };

    for mut svc_ref in svc_store.iter_mut() {
        let svc = if let Service(ref mut svc) = svc_ref.resource {
            svc
        } else {
            continue;
        };

        if selector_match(&svc.spec.selector, &pod.metadata.labels)
            && svc.spec.endpoints.contains(&pod_ip)
        {
            svc.spec.endpoints.retain(|ip| ip != &pod_ip);
            update_service(svc_ref.value()).await?;
        }
    }
    Ok(())
}

async fn update_service(svc: &KubeObject) -> Result<()> {
    let client = reqwest::Client::new();
    client
        .post(format!("{}{}", "http://localhost:8080", svc.uri()))
        .json(svc)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn add_enpoints(
    pod_store: Arc<DashMap<String, KubeObject>>,
    mut svc: KubeObject,
) -> Result<()> {
    let mut svc_changed = false;
    if let Service(ref mut svc) = svc.resource {
        for pod in pod_store.iter_mut() {
            let pod_ip = if let Pod(pod) = &pod.resource {
                if let Some(ip) = pod.get_ip() {
                    ip
                } else {
                    continue;
                }
            } else {
                continue;
            };

            if selector_match(&svc.spec.selector, &pod.metadata.labels) {
                svc_changed = true;
                svc.spec.endpoints.push(pod_ip);
            }
        }
    }

    if svc_changed {
        update_service(&svc).await?
    }
    Ok(())
}
