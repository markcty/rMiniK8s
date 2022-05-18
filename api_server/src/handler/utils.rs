use std::{net::Ipv4Addr, sync::Arc};

use etcd_client::{GetOptions, GetResponse, WatchOptions, WatchStream, Watcher};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use resources::{models::ErrResponse, objects::KubeObject};
use serde::Serialize;

use crate::{
    etcd::{self, kv_to_str},
    AppState,
};

pub async fn etcd_put(
    app_state: &Arc<AppState>,
    key: String,
    val: impl Serialize,
) -> Result<(), ErrResponse> {
    let mut client = app_state.get_client().await?;
    etcd::put(&mut client, &key, val, None)
        .await
        .map_err(ErrResponse::from)?;
    Ok(())
}

pub async fn etcd_get(app_state: &Arc<AppState>, key: String) -> Result<GetResponse, ErrResponse> {
    let mut client = app_state.get_client().await?;
    let res = etcd::get(&mut client, &key, None)
        .await
        .map_err(ErrResponse::from)?;
    Ok(res)
}

pub async fn etcd_delete(app_state: &Arc<AppState>, key: String) -> Result<(), ErrResponse> {
    let mut client = app_state.get_client().await?;
    let res = etcd::delete(&mut client, &key, None)
        .await
        .map_err(ErrResponse::from)?;
    if res.deleted() > 0 {
        Ok(())
    } else {
        Err(ErrResponse::not_found(format!("{} not found", &key), None))
    }
}

pub async fn etcd_get_objects_by_prefix(
    app_state: &Arc<AppState>,
    prefix: String,
    kind: Option<&str>,
) -> Result<Vec<KubeObject>, ErrResponse> {
    let mut client = app_state.get_client().await?;
    let res = etcd::get(&mut client, &prefix, Some(GetOptions::new().with_prefix()))
        .await
        .map_err(ErrResponse::from)?;

    let mut objects: Vec<KubeObject> = Vec::new();
    for kv in res.kvs() {
        let (_, val_str) = kv_to_str(kv)?;
        let object: KubeObject = serde_json::from_str(val_str.as_str()).map_err(|err| {
            ErrResponse::new("failed to deserialize".into(), Some(err.to_string()))
        })?;

        if let Some(kind) = kind {
            if object.kind() == kind {
                objects.push(object);
            } else {
                tracing::error!(
                    "Object kind error: expected {}, found: {}",
                    kind,
                    object.kind()
                );
                return Err(ErrResponse::new(
                    "There are some errors with the kind of objects".to_string(),
                    Some(format!("expected: {}, found: {}", kind, object.kind())),
                ));
            }
        }
    }
    Ok(objects)
}

pub async fn etcd_get_object(
    app_state: &Arc<AppState>,
    uri: String,
    kind: Option<&str>,
) -> Result<KubeObject, ErrResponse> {
    let etcd_res = etcd_get(app_state, uri).await?;
    let object_str = get_value_str(etcd_res)?;
    let object: KubeObject = serde_json::from_str(object_str.as_str())
        .map_err(|err| ErrResponse::new("failed to deserialize".into(), Some(err.to_string())))?;
    if let Some(kind) = kind {
        if object.kind() != kind {
            return Err(ErrResponse::new("get object type error".to_string(), None));
        }
    }
    Ok(object)
}

pub async fn etcd_watch_uri(
    app_state: &Arc<AppState>,
    uri: &str,
) -> Result<(Watcher, WatchStream), ErrResponse> {
    let mut client = app_state.get_client().await?;
    let (watcher, stream) = client
        .watch(uri, Some(WatchOptions::new().with_prefix()))
        .await
        .map_err(|err| {
            ErrResponse::new(
                "Failed to establish watch connection".to_string(),
                Some(err.to_string()),
            )
        })?;
    tracing::info!("Etcd watch created, watch id: {}", watcher.watch_id());
    Ok((watcher, stream))
}

pub fn get_value_str(response: GetResponse) -> Result<String, ErrResponse> {
    if let Some(kv) = response.kvs().first() {
        let (_, val_str) = kv_to_str(kv)?;
        Ok(val_str)
    } else {
        Err(ErrResponse::not_found(
            "value doesn't exist".to_string(),
            None,
        ))
    }
}

pub fn unique_name(name: &str) -> String {
    let mut rng = thread_rng();
    let suffix = (&mut rng)
        .sample_iter(Alphanumeric)
        .take(5)
        .map(char::from)
        .collect::<String>()
        .to_lowercase();
    format!("{}-{}", name, suffix)
}

pub fn gen_service_ip(app_state: &Arc<AppState>) -> Ipv4Addr {
    let mut rng = thread_rng();
    loop {
        let ip = Ipv4Addr::new(172, rng.gen_range(16..32), rng.gen(), rng.gen());
        if !app_state.service_ip_pool.contains(&ip) {
            app_state.service_ip_pool.insert(ip);
            return ip;
        }
    }
}

pub fn gen_rand_host() -> String {
    let mut rng = thread_rng();
    let prefix = (&mut rng)
        .sample_iter(Alphanumeric)
        .take(5)
        .map(char::from)
        .collect::<String>()
        .to_lowercase();
    format!("{}.minik8s.com", prefix)
}
