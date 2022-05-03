use std::sync::Arc;

use axum::{extract::WebSocketUpgrade, response::IntoResponse, Extension, Json};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::KubeObject,
};

use super::{
    response::HandlerResult,
    utils::{etcd_get_objects_by_prefix, etcd_watch_uri},
};
use crate::{etcd::forward_watch_to_ws, AppState};

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
