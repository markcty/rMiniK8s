use std::sync::Arc;

use axum::{extract::Path, Extension, Json};
use axum_macros::debug_handler;
use resources::objects::{pod::PodPhase, KubeObject, KubeResource};

use super::{
    response::{ErrResponse, HandlerResult, Response},
    utils::*,
};
use crate::AppState;

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
    let pods =
        etcd_get_objects_by_prefix(&app_state, "/api/v1/pods".to_string(), Some("pod")).await?;

    let res = Response::new(None, Some(pods));
    Ok(Json(res))
}
