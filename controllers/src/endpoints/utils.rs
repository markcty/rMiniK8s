use anyhow::{anyhow, Ok, Result};
use resources::{
    informer::{EventHandler, Informer, ListerWatcher, ResyncHandler, Store},
    models,
    models::ErrResponse,
    objects::{pod::Pod, service::Service, KubeObject, Object},
};
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;

use crate::{Notification, PodNtf, ResyncNtf, ServiceNtf, CONFIG};

pub fn create_services_informer(
    tx: Sender<Notification>,
    resync_tx: Sender<ResyncNtf>,
) -> Informer<Service> {
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

    let rh = ResyncHandler(Box::new(move |()| {
        let resync_tx = resync_tx.clone();
        Box::pin(async move {
            resync_tx.send(ResyncNtf::Svc).await?;
            Ok(())
        })
    }));

    // start the informer
    Informer::new(lw, eh, rh)
}

pub fn create_pods_informer(
    tx: Sender<Notification>,
    resync_tx: Sender<ResyncNtf>,
) -> Informer<Pod> {
    let lw = ListerWatcher {
        lister: Box::new(|_| {
            Box::pin(async {
                let res = reqwest::get(CONFIG.api_server_endpoint.join("/api/v1/pods")?)
                    .await?
                    .json::<models::Response<Vec<Pod>>>()
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
    let rh = ResyncHandler(Box::new(move |()| {
        let resync_tx = resync_tx.clone();
        Box::pin(async move {
            resync_tx.send(ResyncNtf::Pod).await?;
            Ok(())
        })
    }));

    // start the informer
    Informer::new(lw, eh, rh)
}

pub async fn add_svc_endpoint(svc_store: Store<Service>, pod: Pod) -> Result<()> {
    let pod_ip = if let Some(ip) = pod.get_ip() {
        ip
    } else {
        return Ok(());
    };

    let mut store = svc_store.write().await;
    for (_, svc) in store.iter_mut() {
        if svc.spec.selector.matches(&pod.metadata.labels) && !svc.spec.endpoints.contains(&pod_ip)
        {
            svc.spec.endpoints.insert(pod_ip);
            update_service(svc).await?;
            tracing::info!("Add endpoint {} for service {}", pod_ip, svc.name());
        }
    }
    Ok(())
}

pub async fn del_svc_endpoint(svc_store: Store<Service>, pod: Pod) -> Result<()> {
    let pod_ip = if let Some(ip) = pod.get_ip() {
        ip
    } else {
        return Ok(());
    };

    let mut store = svc_store.write().await;
    for (_, svc) in store.iter_mut() {
        if svc.spec.selector.matches(&pod.metadata.labels) && svc.spec.endpoints.contains(&pod_ip) {
            svc.spec.endpoints.retain(|ip| ip != &pod_ip);
            update_service(svc).await?;
            tracing::info!(
                "Remove endpoint {} for service {}",
                pod_ip,
                svc.metadata.name
            );
        }
    }
    Ok(())
}

pub async fn update_service(svc: &Service) -> Result<()> {
    let client = reqwest::Client::new();
    let res = client
        .put(CONFIG.api_server_endpoint.join(svc.uri().as_str())?)
        .json(&KubeObject::Service(svc.to_owned()))
        .send()
        .await?;
    if res.error_for_status_ref().is_err() {
        let res = res.json::<ErrResponse>().await?;
        tracing::error!("Error update service: {}", res.msg);
    }
    Ok(())
}

pub async fn add_enpoints(pod_store: Store<Pod>, mut svc: Service) -> Result<()> {
    let mut svc_changed = false;
    let mut store = pod_store.write().await;
    for (_, pod) in store.iter_mut() {
        let pod_ip = if let Some(ip) = pod.get_ip() {
            ip
        } else {
            continue;
        };

        if svc.spec.selector.matches(&pod.metadata.labels) {
            svc_changed = true;
            svc.spec.endpoints.insert(pod_ip);
            tracing::info!("Add endpoint {} for service {}", pod_ip, svc.metadata.name);
        }
    }

    if svc_changed {
        update_service(&svc).await?
    }
    Ok(())
}
