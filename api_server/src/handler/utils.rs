use std::sync::Arc;

use etcd_client::{GetOptions, GetResponse};
use resources::objects::pod::{Pod, PodCondition, PodConditionType, PodPhase, PodStatus};
use serde::Serialize;

use super::response::ErrResponse;
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
    etcd::put(&mut client, key, val)
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

pub async fn etcd_get_prefix(
    app_state: &Arc<AppState>,
    prefix: String,
) -> Result<GetResponse, ErrResponse> {
    let mut client = app_state.get_client().await?;
    let res = etcd::get(&mut client, prefix, Some(GetOptions::new().with_prefix()))
        .await
        .map_err(ErrResponse::from)?;
    Ok(res)
}

pub async fn get_pod_from_etcd(
    app_state: &Arc<AppState>,
    pod_name: String,
) -> Result<Pod, ErrResponse> {
    let pod_uri = format!("/api/v1/pods/{}", pod_name);
    let etcd_res = etcd_get(app_state, pod_uri).await?;
    let pod_str = get_value_str(etcd_res)?;
    let pod: Pod = serde_json::from_str(pod_str.as_str())
        .map_err(|err| ErrResponse::new("failed to deserialize".into(), Some(err.to_string())))?;
    Ok(pod)
}

pub async fn put_pod_to_etcd(
    app_state: &Arc<AppState>,
    pod_name: String,
    pod: Pod,
) -> Result<(), ErrResponse> {
    let pod_uri = format!("/api/v1/pods/{}", pod_name);
    etcd_put(app_state, pod_uri, pod).await?;
    Ok(())
}

pub fn get_value_str(reponse: GetResponse) -> Result<String, ErrResponse> {
    if let Some(kv) = reponse.kvs().first() {
        let (_, val_str) = kv_to_str(kv)?;
        Ok(val_str)
    } else {
        Err(ErrResponse::new("value didn't exist".to_string(), None))
    }
}

pub fn update_pod_phase(pod: &mut Pod, new_phase: PodPhase) {
    if let Some(ref mut status) = pod.status {
        status.phase = new_phase;
    } else {
        let new_status: PodStatus = PodStatus {
            phase: new_phase,
            ..PodStatus::default()
        };
        pod.status = Some(new_status);
    }
}

pub fn update_pod_condition(
    pod: &mut Pod,
    new_condition_type: PodConditionType,
    new_condition: PodCondition,
) {
    if let Some(ref mut status) = pod.status {
        status.conditions.insert(new_condition_type, new_condition);
    } else {
        let mut new_status: PodStatus = PodStatus::default();
        new_status
            .conditions
            .insert(new_condition_type, new_condition);
        pod.status = Some(new_status);
    }
}
