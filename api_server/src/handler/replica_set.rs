use std::sync::Arc;

use axum::{
    extract::{Path, WebSocketUpgrade},
    response::IntoResponse,
    Extension, Json,
};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{replica_set::ReplicaSetStatus, KubeObject, Object},
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
    if let KubeObject::ReplicaSet(ref mut rs) = payload {
        let rs_name = &rs.metadata.name.to_owned();
        let result = etcd_get_object(&app_state, rs.uri(), Some("replicaset")).await;
        if result.is_ok() {
            return Err(ErrResponse::new(
                String::from("Error creating replica set"),
                Some(format!("Replica set {} already exists", rs_name)),
            ));
        }
        rs.metadata.uid = Some(Uuid::new_v4());
        rs.status = Some(ReplicaSetStatus::default());

        etcd_put(&app_state, &payload).await?;
        let res = Response::new(Some(format!("replicaset/{} created", rs_name)), None);
        Ok(Json(res))
    } else {
        // TODO: fill business logic and error handling
        return Err(ErrResponse::new(
            String::from("Error creating replicaset"),
            Some(format!("Expecting replicaset kind, got {}", payload.kind())),
        ));
    }
}

#[debug_handler]
pub async fn get(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(rs_name): Path<String>,
) -> HandlerResult<KubeObject> {
    let rs = etcd_get_object(
        &app_state,
        format!("/api/v1/replicasets/{}", rs_name),
        Some("replicaset"),
    )
    .await?;
    let res = Response::new(None, Some(rs));
    Ok(Json(res))
}

#[debug_handler]
pub async fn delete(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(rs_name): Path<String>,
) -> HandlerResult<()> {
    etcd_delete(&app_state, format!("/api/v1/replicasets/{}", rs_name)).await?;
    let res = Response::new(Some(format!("replicaset/{} deleted", rs_name)), None);
    Ok(Json(res))
}

#[debug_handler]
pub async fn update(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(rs_name): Path<String>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    if let KubeObject::ReplicaSet(_) = payload {
        etcd_put(&app_state, &payload).await?;
        let res = Response::new(Some(format!("replicaset/{} updated", rs_name)), None);
        Ok(Json(res))
    } else {
        Err(ErrResponse::new(
            String::from("Error updating replicaset"),
            Some(format!("Expecting replicaset kind, got {}", payload.kind())),
        ))
    }
}

#[debug_handler]
pub async fn patch(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(rs_name): Path<String>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    let mut object = etcd_get_object(
        &app_state,
        format!("/api/v1/replicasets/{}", rs_name),
        Some("replicaset"),
    )
    .await?;
    match (&payload, &mut object) {
        (KubeObject::ReplicaSet(payload_rs), KubeObject::ReplicaSet(ref mut rs)) => {
            rs.spec = payload_rs.spec.to_owned();
            etcd_put(&app_state, &object).await?;
            let res = Response::new(Some(format!("replicaset/{} patched", rs_name)), None);
            Ok(Json(res))
        },
        _ => Err(ErrResponse::new(
            String::from("Error patching replicaset"),
            Some(format!("Expecting replicaset kind, got {}", payload.kind())),
        )),
    }
}

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
) -> HandlerResult<Vec<KubeObject>> {
    let replicasets = etcd_get_objects_by_prefix(
        &app_state,
        "/api/v1/replicasets".to_string(),
        Some("replicaset"),
    )
    .await?;

    let res = Response::new(None, Some(replicasets));
    Ok(Json(res))
}

#[debug_handler]
pub async fn watch_all(
    Extension(app_state): Extension<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ErrResponse> {
    // open etcd watch connection
    let (watcher, stream) = etcd_watch_uri(&app_state, "/api/v1/replicasets").await?;

    Ok(ws.on_upgrade(|socket| async move {
        forward_watch_to_ws::<KubeObject>(socket, watcher, stream).await
    }))
}
