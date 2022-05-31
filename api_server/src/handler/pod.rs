use std::sync::Arc;

use axum::{
    extract::{OriginalUri, Path, WebSocketUpgrade},
    http::Request,
    response::IntoResponse,
    Extension, Json,
};
use axum_macros::debug_handler;
use hyper::Body;
use resources::{
    models::{ErrResponse, Response},
    objects::{pod::PodPhase, KubeObject, Object},
};
use uuid::Uuid;

use super::{response::HandlerResult, utils::*};
use crate::{etcd::forward_watch_to_ws, AppState};

#[debug_handler]
pub async fn create(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(mut payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // TODO: validate payload
    if let KubeObject::Pod(ref mut pod) = payload {
        pod.metadata.uid = Some(Uuid::new_v4());
        pod.metadata.name = unique_name(&pod.metadata.name);
        let pod_name = pod.metadata.name.to_owned();

        let mut status = pod.status.clone().unwrap_or_default();
        status.phase = PodPhase::Pending;
        pod.status = Some(status);

        etcd_put(&app_state, &payload).await?;
        let res = Response::new(Some(format!("pod/{} created", pod_name)), None);
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
    // Ensure object exists, otherwise deleted object will be created
    etcd_get_object(
        &app_state,
        format!("/api/v1/pods/{}", pod_name),
        Some("pod"),
    )
    .await?;
    if let KubeObject::Pod(_) = payload {
        etcd_put(&app_state, &payload).await?;
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

#[debug_handler]
pub async fn pod_logs(
    Extension(app_state): Extension<Arc<AppState>>,
    OriginalUri(uri): OriginalUri,
    Path(pod_name): Path<String>,
    request: Request<Body>,
) -> axum::http::Response<Body> {
    proxy_to_rkubelet(app_state, uri, pod_name, request).await
}

#[debug_handler]
pub async fn container_logs(
    Extension(app_state): Extension<Arc<AppState>>,
    OriginalUri(uri): OriginalUri,
    Path((pod_name, _)): Path<(String, String)>,
    request: Request<Body>,
) -> axum::http::Response<Body> {
    proxy_to_rkubelet(app_state, uri, pod_name, request).await
}
