use std::sync::Arc;

use axum::{extract::Path, Extension, Json};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{KubeObject, KubeResource, Object},
};
use uuid::Uuid;

use super::{response::HandlerResult, utils::*};
use crate::AppState;

#[debug_handler]
pub async fn create(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(mut payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // TODO: validate payload
    if let KubeResource::Service(_) = payload.resource {
        payload.metadata.uid = Some(Uuid::new_v4());

        etcd_put(&app_state, payload.uri(), &payload).await?;
        let res = Response::new(Some(format!("service/{} created", payload.name())), None);
        Ok(Json(res))
    } else {
        // TODO: fill business logic and error handling
        return Err(ErrResponse::new(
            String::from("Error creating service"),
            Some(format!("Expecting service, got {}", payload.kind())),
        ));
    }
}

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
) -> HandlerResult<Vec<KubeObject>> {
    let services =
        etcd_get_objects_by_prefix(&app_state, "/api/v1/services".to_string(), Some("Service"))
            .await?;

    let res = Response::new(None, Some(services));
    Ok(Json(res))
}

#[debug_handler]
pub async fn get(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
) -> HandlerResult<KubeObject> {
    let service = etcd_get_object(
        &app_state,
        format!("/api/v1/services/{}", name),
        Some("Service"),
    )
    .await?;
    let res = Response::new(None, Some(service));
    Ok(Json(res))
}

#[debug_handler]
pub async fn delete(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
) -> HandlerResult<()> {
    etcd_delete(&app_state, format!("/api/v1/services/{}", name)).await?;
    let res = Response::new(Some(format!("services/{} deleted", name)), None);
    Ok(Json(res))
}
