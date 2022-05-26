use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Multipart, Path, Query, WebSocketUpgrade},
    response::IntoResponse,
    Extension, Json,
};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{function::Function, service::Service, KubeObject, Object},
};

use super::{response::HandlerResult, utils::*};
use crate::{etcd::forward_watch_to_ws, AppState};

#[debug_handler]
pub async fn create(
    Extension(app_state): Extension<Arc<AppState>>,
    Query(param): Query<HashMap<String, String>>,
    mut multipart: Multipart,
) -> HandlerResult<()> {
    // get function name
    let name = if let Some(name) = param.get("name") {
        name.to_owned()
    } else {
        let err = ErrResponse::bad_request("function name should be specified".to_string(), None);
        return Err(err);
    };

    // get file and store in tmp
    let field = if let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ErrResponse::new(e.to_string(), None))?
    {
        field
    } else {
        let err = ErrResponse::bad_request("File upload failed".to_string(), None);
        return Err(err);
    };
    let original_filename = field.file_name().map_or("".to_string(), |n| n.to_owned());
    if !original_filename.ends_with(".zip") {
        let err = ErrResponse::bad_request("Please upload a zip file".to_string(), None);
        return Err(err);
    }
    let filename = stream_to_tmp_file(original_filename.as_str(), field).await?;

    // create service object
    let svc_name = unique_name("func-svc");
    let service = KubeObject::Service(Service::from_function(
        &svc_name,
        name.as_str(),
        gen_service_ip(&app_state),
    ));
    etcd_put(&app_state, &service).await?;

    // create function object
    let function = KubeObject::Function(Function::new(name.to_owned(), service.uri(), filename));
    etcd_put(&app_state, &function).await?;

    let mut i = 0;
    loop {
        let function = etcd_get_object(
            &app_state,
            format!("/api/v1/functions/{}", name),
            Some("function"),
        )
        .await?;
        if let KubeObject::Function(function) = function {
            if let Some(image) = function.status.image {
                tracing::info!("Image for function {} is created, name: {}", name, image);
                break;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        if i % 5 == 0 {
            tracing::info!("Image creation for function {} is still pending", name);
        }
        if i == 60 {
            tracing::warn!("Image creation for function {} timeout", name);
            return Err(ErrResponse::new(
                format!("Image creation for function {} timeout", name),
                None,
            ));
        }
        i += 1;
    }

    let res = Response::new(Some(format!("function/{} created", name)), None);
    Ok(Json(res))
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
        etcd_delete(
            &app_state,
            format!("/api/v1/services/{}", func.spec.service_ref),
        )
        .await?;
    }
    etcd_delete(&app_state, format!("/api/v1/functions/{}", name)).await?;
    let res = Response::new(Some(format!("functions/{} deleted", name)), None);
    Ok(Json(res))
}
