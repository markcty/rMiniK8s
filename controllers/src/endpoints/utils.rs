use std::{net::Ipv4Addr, sync::Arc};

use anyhow::{anyhow, Ok, Result};
use dashmap::DashMap;
use resources::{
    informer::{EventHandler, Informer, ListerWatcher},
    models,
    models::ErrResponse,
    objects::{
        KubeObject,
        KubeResource::{Pod, Service},
        Object,
    },
    utils::selector_match,
};
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;

use crate::{Notification, PodNtf, ServiceNtf, CONFIG};

pub fn create_services_informer(
    tx: Sender<Notification>,
) -> (Informer<KubeObject>, Arc<DashMap<String, KubeObject>>) {
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get(CONFIG.api_server_endpoint.join("/api/v1/services")?)
                    .await?
                    .json::<models::Response<Vec<KubeObject>>>()
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
                let res = reqwest::get(CONFIG.api_server_endpoint.join("/api/v1/pods")?)
                    .await?
                    .json::<models::Response<Vec<KubeObject>>>()
                    .await?;
                let res = res.data.ok_or_else(|| anyhow!("Lister failed"))?;
                Ok(res)
            })
        }),
        watcher: Box::new(|_| {
            Box::pin(async {
                let mut url = CONFIG.api_server_endpoint.join("/api/v1/watch/pods")?;
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
            tracing::info!(
                "Add endpoint {} for service {}",
                pod_ip,
                svc_ref.metadata.name
            );
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
            tracing::info!(
                "Remove endpoint {} for service {}",
                pod_ip,
                svc_ref.metadata.name
            );
        }
    }
    Ok(())
}

async fn update_service(svc: &KubeObject) -> Result<()> {
    let client = reqwest::Client::new();
    let res = client
        .put(CONFIG.api_server_endpoint.join(svc.uri().as_str())?)
        .json(svc)
        .send()
        .await?;
    if res.error_for_status_ref().is_err() {
        let res = res.json::<ErrResponse>().await?;
        tracing::error!("Error update service: {}", res.msg);
    }
    Ok(())
}

pub async fn add_enpoints(
    pod_store: Arc<DashMap<String, KubeObject>>,
    mut svc: KubeObject,
) -> Result<()> {
    let mut svc_changed = false;
    if let Service(ref mut svc_res) = svc.resource {
        for pod in pod_store.iter_mut() {
            let pod_ip = if let Some(ip) = get_pod_ip(pod.value()) {
                ip
            } else {
                continue;
            };

            if selector_match(&svc_res.spec.selector, &pod.metadata.labels) {
                svc_changed = true;
                svc_res.spec.endpoints.push(pod_ip);
                tracing::info!("Add endpoint {} for service {}", pod_ip, svc.metadata.name);
            }
        }
    }

    if svc_changed {
        update_service(&svc).await?
    }
    Ok(())
}

fn get_pod_ip(pod: &KubeObject) -> Option<Ipv4Addr> {
    if let Pod(pod) = &pod.resource {
        Some(pod.get_ip()).flatten()
    } else {
        None
    }
}

pub fn pod_ip_changed(pod1: &KubeObject, pod2: &KubeObject) -> bool {
    let pod_ip1 = get_pod_ip(pod1);
    let pod_ip2 = get_pod_ip(pod2);
    pod_ip1 != pod_ip2
}
