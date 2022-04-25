use std::sync::Arc;

use axum::{
    extract::{Path, WebSocketUpgrade},
    http::Uri,
    response::IntoResponse,
    Extension, Json,
};
use axum_macros::debug_handler;
use etcd_client::*;
use resources::objects::KubeObject;

use super::{
    response::{HandlerResult, Response},
    utils::etcd_put,
};
use crate::{etcd::forward_watch_to_ws, handler::response::ErrResponse, AppState};

#[debug_handler]
pub async fn apply(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(pod_name): Path<String>,
    Json(payload): Json<KubeObject>,
    uri: Uri,
) -> HandlerResult<()> {
    // TODO: validate payload and add status
    etcd_put(&app_state, uri.to_string(), payload.clone()).await?;
    let res = Response::new(format!("pod/{} created", pod_name), None);

    // TODO: fill business logic and error handling
    let queue = &mut app_state.schedule_queue.write().unwrap();
    queue.push(payload);
    Ok(Json(res))
}

#[debug_handler]
pub async fn watch_all(
    Extension(app_state): Extension<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ErrResponse> {
    // open etcd watch connection
    let mut client = app_state.get_client().await?;
    let (watcher, stream) = client
        .watch("/api/v1/pods", Some(WatchOptions::new().with_prefix()))
        .await
        .map_err(|err| {
            ErrResponse::new(
                "Failed to establish watch connection".to_string(),
                Some(err.to_string()),
            )
        })?;
    tracing::info!("Etcd watch created, watch id: {}", watcher.watch_id());

    Ok(ws.on_upgrade(|socket| async move { forward_watch_to_ws(socket, watcher, stream).await }))
}
