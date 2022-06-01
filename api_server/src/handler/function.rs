use std::sync::Arc;

use axum::{
    extract::{Multipart, Path, WebSocketUpgrade},
    response::IntoResponse,
    Extension, Json,
};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{service::Service, KubeObject, Object},
};

use super::{response::HandlerResult, utils::*};
use crate::{etcd::forward_watch_to_ws, AppState};

#[debug_handler]
pub async fn create(
    Extension(app_state): Extension<Arc<AppState>>,
    mut multipart: Multipart,
) -> HandlerResult<()> {
    let mut payload: Option<KubeObject> = None;
    let mut filename: Option<String> = None;
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().map_or("".to_string(), |n| n.to_owned());
        match name.as_str() {
            "function" => {
                payload = Some(decode_field_json(field).await?);
            },
            "code" => {
                filename = Some(store_code_file(field).await?);
            },
            _ => {},
        }
    }

    let mut payload = payload.ok_or_else(|| {
        ErrResponse::bad_request("Function field is not presented".to_string(), None)
    })?;
    let filename = filename
        .ok_or_else(|| ErrResponse::bad_request("Code field is not presented".to_string(), None))?;

    if let KubeObject::Function(ref mut function) = payload {
        // get function name
        let name = function.metadata.name.to_owned();

        // create service object
        let svc_name = unique_name(&format!("func-{}", name));
        let service = KubeObject::Service(Service::from_function(
            &svc_name,
            name.as_str(),
            gen_service_ip(&app_state),
        ));
        etcd_put(&app_state, &service).await?;

        // create function object
        function.init(service.uri(), filename);
        etcd_put(&app_state, &payload).await?;

        let mut i = 0;
        loop {
            let function = etcd_get_object(&app_state, payload.uri(), Some(payload.kind())).await?;
            if let KubeObject::Function(function) = function {
                if let Some(image) = function.status.unwrap().image {
                    tracing::info!(
                        "Image for function {} has been created, name: {}",
                        name,
                        image
                    );
                    break;
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            if i % 5 == 0 {
                tracing::info!("Image creation for function {} is still pending", name);
            }
            if i == 60 {
                tracing::warn!("Image creation for function {} timed out", name);
                return Err(ErrResponse::new(
                    format!("Image creation for function {} timed out", name),
                    None,
                ));
            }
            i += 1;
        }

        let res = Response::new(Some(format!("function/{} created", name)), None);
        Ok(Json(res))
    } else {
        Err(ErrResponse::new(
            String::from("Error creating function"),
            Some(format!("Expecting function kind, got {}", payload.kind())),
        ))
    }
}

#[debug_handler]
pub async fn watch_all(
    Extension(app_state): Extension<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ErrResponse> {
    // open etcd watch connection
    let (watcher, stream) = etcd_watch_uri(&app_state, "/api/v1/functions").await?;

    Ok(ws.on_upgrade(|socket| async move {
        forward_watch_to_ws::<KubeObject>(socket, watcher, stream).await
    }))
}

#[debug_handler]
pub async fn watch_one(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ErrResponse> {
    // open etcd watch connection
    let (watcher, stream) =
        etcd_watch_uri(&app_state, &format!("/api/v1/functions/{}", name)).await?;

    Ok(ws.on_upgrade(|socket| async move {
        forward_watch_to_ws::<KubeObject>(socket, watcher, stream).await
    }))
}

#[debug_handler]
pub async fn get(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
) -> HandlerResult<KubeObject> {
    let function = etcd_get_object(
        &app_state,
        format!("/api/v1/functions/{}", name),
        Some("function"),
    )
    .await?;
    let res = Response::new(None, Some(function));
    Ok(Json(res))
}

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
) -> HandlerResult<Vec<KubeObject>> {
    let functions = etcd_get_objects_by_prefix(
        &app_state,
        "/api/v1/functions".to_string(),
        Some("function"),
    )
    .await?;

    let res = Response::new(None, Some(functions));
    Ok(Json(res))
}

#[debug_handler]
pub async fn update(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // TODO: validate payload
    if let KubeObject::Function(_) = payload {
        etcd_put(&app_state, &payload).await?;
        let res = Response::new(Some(format!("functions/{} updated", payload.name())), None);
        Ok(Json(res))
    } else {
        // TODO: fill business logic and error handling
        return Err(ErrResponse::new(
            String::from("Error creating function"),
            Some(format!("Expecting function, got {}", payload.kind())),
        ));
    }
}

#[debug_handler]
pub async fn delete(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
) -> HandlerResult<()> {
    let function = etcd_get_object(
        &app_state,
        format!("/api/v1/functions/{}", name),
        Some("function"),
    )
    .await?;
    if let KubeObject::Function(func) = function {
        if let Some(status) = func.status {
            etcd_delete(&app_state, status.service_ref)
                .await
                .unwrap_or_else(|_| tracing::error!("Error deleting service"));
        }
        etcd_delete(&app_state, format!("/api/v1/replicasets/{}", name))
            .await
            .unwrap_or_else(|_| tracing::error!("Error deleting replicaset"));
        etcd_delete(
            &app_state,
            format!("/api/v1/horizontalpodautoscalers/{}", name),
        )
        .await
        .unwrap_or_else(|_| tracing::error!("Error deleting HPA"));
    }
    etcd_delete(&app_state, format!("/api/v1/functions/{}", name)).await?;
    let res = Response::new(Some(format!("functions/{} deleted", name)), None);
    Ok(Json(res))
}
