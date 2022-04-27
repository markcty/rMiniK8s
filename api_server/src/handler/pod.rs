use std::sync::Arc;

use axum::{extract::Path, Extension, Json};
use axum_macros::debug_handler;
use resources::objects::{pod::PodPhase, KubeObject, KubeResource};

use super::{
    response::{ErrResponse, HandlerResult, Response},
    utils::*,
};
use crate::{etcd::kv_to_str, AppState};

#[debug_handler]
pub async fn apply(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(pod_name): Path<String>,
    Json(mut payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // TODO: validate payload
    // Because we have only one type in KubeResource now,
    // there will be a warning temporarily.
    #[allow(irrefutable_let_patterns)]
    if let KubeResource::Pod(ref mut pod) = payload.resource {
        let mut status = pod.status.clone().unwrap_or_default();
        status.phase = PodPhase::Pending;
        pod.status = Some(status);
        etcd_put(
            &app_state,
            format!("/api/v1/pods/{}", pod_name),
            payload.clone(),
        )
        .await?;
        let res = Response::new(Some(format!("pod/{} created", pod_name)), None);
        let queue = &mut app_state.schedule_queue.write().unwrap();
        queue.push(payload);
        Ok(Json(res))
    } else {
        // TODO: fill business logic and error handling
        return Err(ErrResponse::new(
            String::from("apply error"),
            Some(format!(
                "the kind of payload: {}, which is not pod",
                payload.kind()
            )),
        ));
    }
}

#[debug_handler]
pub async fn get(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(pod_name): Path<String>,
) -> HandlerResult<KubeObject> {
    let pod_object = etcd_get_object(
        &app_state,
        format!("/api/v1/pods/{}", pod_name),
        Some("pod"),
    )
    .await?;
    let res = Response::new(None, Some(pod_object));
    Ok(Json(res))
}

#[debug_handler]
pub async fn replace(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(pod_name): Path<String>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    if payload.kind() == "pod" {
        etcd_put(
            &app_state,
            format!("/api/v1/pods/{}", pod_name),
            payload.clone(),
        )
        .await?;
        let res = Response::new(Some(format!("pod/{} replaced", pod_name)), None);
        Ok(Json(res))
    } else {
        Err(ErrResponse::new(
            String::from("replace error"),
            Some(format!(
                "the kind of payload: {}, which is not pod",
                payload.kind()
            )),
        ))
    }
}

#[debug_handler]
pub async fn delete(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(pod_name): Path<String>,
) -> HandlerResult<()> {
    etcd_delete(&app_state, format!("/api/v1/pods/{}", pod_name)).await?;
    let res = Response::new(Some(format!("pod/{} deleted", pod_name)), None);
    Ok(Json(res))
}

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
) -> HandlerResult<Vec<KubeObject>> {
    // uri: /api/v1/pods
    let etcd_res = etcd_get_objects_by_prefix(&app_state, "/api/v1/pods".to_string()).await?;
    let mut pods: Vec<KubeObject> = Vec::new();
    let mut msg = "get pods successfully".to_string();
    for kv in etcd_res.kvs() {
        let (_, val_str) = kv_to_str(kv)?;
        let pod: KubeObject = serde_json::from_str(val_str.as_str()).map_err(|err| {
            ErrResponse::new("failed to deserialize".into(), Some(err.to_string()))
        })?;
        if pod.kind() == "pod" {
            pods.push(pod);
        } else {
            // only report the error, then continue to process other objects
            tracing::error!("There are some errors with the kind of objects");
            msg = "There are some errors with the kind of objects".to_string();
        }
    }
    let res = Response::new(Some(msg), Some(pods));
    Ok(Json(res))
}
