use std::sync::Arc;

use etcd_client::{GetOptions, GetResponse};
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
    etcd::put(&mut client, key, val, None)
        .await
        .map_err(ErrResponse::from)?;
    Ok(())
}

pub async fn etcd_get(app_state: &Arc<AppState>, key: String) -> Result<GetResponse, ErrResponse> {
    let mut client = app_state.get_client().await?;
    let res = etcd::get(&mut client, key, None)
        .await
        .map_err(ErrResponse::from)?;
    Ok(res)
}

pub async fn etcd_delete(app_state: &Arc<AppState>, key: String) -> Result<(), ErrResponse> {
    let mut client = app_state.get_client().await?;
    etcd::delete(&mut client, key, None)
        .await
        .map_err(ErrResponse::from)?;
    Ok(())
}

pub async fn etcd_get_objects_by_prefix(
    app_state: &Arc<AppState>,
    prefix: String,
    kind: Option<&str>,
) -> Result<Vec<KubeObject>, ErrResponse> {
    let mut client = app_state.get_client().await?;
    let res = etcd::get(&mut client, prefix, Some(GetOptions::new().with_prefix()))
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
                tracing::error!("There are some errors with the kind of objects");
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

pub fn get_value_str(reponse: GetResponse) -> Result<String, ErrResponse> {
    if let Some(kv) = reponse.kvs().first() {
        let (_, val_str) = kv_to_str(kv)?;
        Ok(val_str)
    } else {
        Err(ErrResponse::new("value didn't exist".to_string(), None))
    }
}

pub fn unique_pod_name(name: &str) -> String {
    let mut rng = thread_rng();
    let suffix = (&mut rng)
        .sample_iter(Alphanumeric)
        .take(5)
        .map(char::from)
        .collect::<String>()
        .to_lowercase();
    format!("{}-{}", name, suffix)
}
