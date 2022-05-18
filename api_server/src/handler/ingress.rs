use std::sync::Arc;

use axum::{
    extract::{Path, WebSocketUpgrade},
    response::IntoResponse,
    Extension, Json,
};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{KubeObject, KubeResource, Object},
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
    if let KubeResource::Ingress(ref mut ingress) = payload.resource {
        payload.metadata.uid = Some(Uuid::new_v4());

        for rule in ingress.spec.rules.iter_mut() {
            if rule.host.is_none() {
                rule.host = Some(gen_rand_host());
            }
        }

        etcd_put(&app_state, payload.uri(), &payload).await?;
        let res = Response::new(Some(format!("ingress/{} created", payload.name())), None);
        Ok(Json(res))
    } else {
        // TODO: fill business logic and error handling
        return Err(ErrResponse::new(
            String::from("Error creating ingress"),
            Some(format!("Expecting ingress, got {}", payload.kind())),
        ));
    }
}

#[debug_handler]
pub async fn update(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // TODO: validate payload
    if let KubeResource::Ingress(_) = payload.resource {
        etcd_put(&app_state, payload.uri(), &payload).await?;
        let res = Response::new(Some(format!("ingress/{} updated", payload.name())), None);
        Ok(Json(res))
    } else {
        // TODO: fill business logic and error handling
        return Err(ErrResponse::new(
            String::from("Error creating ingress"),
            Some(format!("Expecting ingress, got {}", payload.kind())),
        ));
    }
}

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
) -> HandlerResult<Vec<KubeObject>> {
    let ingresses =
        etcd_get_objects_by_prefix(&app_state, "/api/v1/ingresses".to_string(), Some("ingress"))
            .await?;

    let res = Response::new(None, Some(ingresses));
    Ok(Json(res))
}

#[debug_handler]
pub async fn get(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
) -> HandlerResult<KubeObject> {
    let ingress = etcd_get_object(
        &app_state,
        format!("/api/v1/ingresses/{}", name),
        Some("ingress"),
    )
    .await?;
    let res = Response::new(None, Some(ingress));
    Ok(Json(res))
}

#[debug_handler]
pub async fn delete(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
) -> HandlerResult<()> {
    etcd_delete(&app_state, format!("/api/v1/ingresses/{}", name)).await?;
    let res = Response::new(Some(format!("ingresses/{} deleted", name)), None);
    Ok(Json(res))
}

#[debug_handler]
pub async fn watch_all(
    Extension(app_state): Extension<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ErrResponse> {
    // open etcd watch connection
    let (watcher, stream) = etcd_watch_uri(&app_state, "/api/v1/ingresses").await?;

    Ok(ws.on_upgrade(|socket| async move {
        forward_watch_to_ws::<KubeObject>(socket, watcher, stream).await
    }))
}
