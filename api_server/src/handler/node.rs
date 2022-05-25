use std::sync::Arc;

use axum::{
    extract::{Path, WebSocketUpgrade},
    response::IntoResponse,
    Extension, Json,
};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{KubeObject, Object},
};
use uuid::Uuid;

use super::{
    response::HandlerResult,
    utils::{etcd_get_objects_by_prefix, etcd_watch_uri},
};
use crate::{
    etcd::forward_watch_to_ws,
    handler::utils::{etcd_delete, etcd_get_object, etcd_put},
    AppState,
};

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
) -> HandlerResult<Vec<KubeObject>> {
    let nodes =
        etcd_get_objects_by_prefix(&app_state, "/api/v1/nodes".to_string(), Some("node")).await?;

    let res = Response::new(None, Some(nodes));
    Ok(Json(res))
}

#[debug_handler]
pub async fn get(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(node_name): Path<String>,
) -> HandlerResult<KubeObject> {
    let node = etcd_get_object(
        &app_state,
        format!("/api/v1/nodes/{}", node_name),
        Some("node"),
    )
    .await?;
    let res = Response::new(None, Some(node));
    Ok(Json(res))
}

#[debug_handler]
pub async fn update(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(node_name): Path<String>,
    Json(mut payload): Json<KubeObject>,
) -> HandlerResult<KubeObject> {
    if let KubeObject::Node(ref mut node) = payload {
        let old_node = etcd_get_object(
            &app_state,
            format!("/api/v1/nodes/{}", node_name),
            Some("node"),
        )
        .await;
        match old_node {
            // Node exists, replace status and return existing metadata
            Ok(KubeObject::Node(old_node)) => {
                node.metadata = old_node.metadata;
            },
            _ => {
                node.metadata.uid = Some(Uuid::new_v4());
            },
        }
        etcd_put(&app_state, &payload).await?;
        let res = Response::new(Some(format!("node/{} updated", node_name)), Some(payload));
        Ok(Json(res))
    } else {
        Err(ErrResponse::new(
            String::from("Error updating node"),
            Some(format!("Expecting node kind, got {}", payload.kind())),
        ))
    }
}

#[debug_handler]
pub async fn delete(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(node_name): Path<String>,
) -> HandlerResult<()> {
    etcd_delete(&app_state, format!("/api/v1/nodes/{}", node_name)).await?;
    let res = Response::new(Some(format!("node/{} deleted", node_name)), None);
    Ok(Json(res))
}

#[debug_handler]
pub async fn watch_all(
    Extension(app_state): Extension<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ErrResponse> {
    // open etcd watch connection
    let (watcher, stream) = etcd_watch_uri(&app_state, "/api/v1/nodes").await?;

    Ok(ws.on_upgrade(|socket| async move {
        forward_watch_to_ws::<KubeObject>(socket, watcher, stream).await
    }))
}
