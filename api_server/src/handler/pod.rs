use std::sync::Arc;

use axum::{
    extract::{Path, WebSocketUpgrade},
    response::IntoResponse,
    Extension, Json,
};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{pod::PodPhase, KubeObject, KubeResource},
};
use uuid::Uuid;

use super::{response::HandlerResult, utils::*};
use crate::{etcd::forward_watch_to_ws, AppState};

#[debug_handler]
pub async fn create(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(pod_name): Path<String>,
    Json(mut payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // TODO: validate payload
    if let KubeResource::Pod(ref mut pod) = payload.resource {
        payload.metadata.uid = Some(Uuid::new_v4());
        payload.metadata.name = unique_pod_name(&pod_name);
        let pod_name = &payload.metadata.name;

        let mut status = pod.status.clone().unwrap_or_default();
        status.phase = PodPhase::Pending;
        pod.status = Some(status);
        etcd_put(&app_state, format!("/api/v1/pods/{}", pod_name), &payload).await?;
        let res = Response::new(Some(format!("pod/{} created", pod_name)), None);
        let queue = &mut app_state.schedule_queue.write().unwrap();
        queue.push(payload);
        Ok(Json(res))
    } else {
        // TODO: fill business logic and error handling
        return Err(ErrResponse::new(
            String::from("Error creating pod"),
            Some(format!("Expecting pod kind, got {}", payload.kind())),
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
        etcd_put(&app_state, format!("/api/v1/pods/{}", pod_name), &payload).await?;
        let res = Response::new(Some(format!("pod/{} replaced", pod_name)), None);
        Ok(Json(res))
    } else {
        Err(ErrResponse::new(
            String::from("Error replacing pod"),
            Some(format!("Expecting pod kind, got {}", payload.kind())),
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

#[debug_handler]
pub async fn watch_all(
    Extension(app_state): Extension<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ErrResponse> {
    // open etcd watch connection
    let (watcher, stream) = etcd_watch_uri(&app_state, "/api/v1/pods").await?;

    Ok(ws.on_upgrade(|socket| async move {
        forward_watch_to_ws::<KubeObject>(socket, watcher, stream).await
    }))
}
