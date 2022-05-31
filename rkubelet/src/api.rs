use std::sync::Arc;

use axum::{
    extract::{Path, Query},
    Extension, Json,
};
use axum_macros::debug_handler;
use resources::models::{ErrResponse, Response};
use serde::{Deserialize, Serialize};

use crate::{docker::Container, pod::Pod, AppState};

#[debug_handler]
pub async fn container_logs(
    Extension(app_state): Extension<Arc<AppState>>,
    Path((pod_name, container_name)): Path<(String, String)>,
    query: Query<LogsQuery>,
) -> Result<Json<Response<String>>, ErrResponse> {
    let pods = app_state.pod_store.read().await;
    let pod = pods
        .get(&format!("/api/v1/pods/{}", pod_name))
        .ok_or_else(|| {
            ErrResponse::new(
                String::from("Pod not found"),
                Some(format!("Pod {} not found", pod_name)),
            )
        })?;
    let uid = pod
        .metadata
        .uid
        .ok_or_else(|| ErrResponse::new(String::from("Pod has no uid"), None))?;
    let container = Container::new(Pod::unique_container_name(&uid, &pod_name, &container_name));
    let logs = container.logs(&query.tail).await.map_err(|err| {
        ErrResponse::new(
            String::from("Failed to get container logs"),
            Some(err.to_string()),
        )
    })?;
    Ok(Json(Response::new(None, Some(logs))))
}

#[debug_handler]
pub async fn pod_logs(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(pod_name): Path<String>,
    query: Query<LogsQuery>,
) -> Result<Json<Response<String>>, ErrResponse> {
    let pods = app_state.pod_store.read().await;
    let pod = pods
        .get(&format!("/api/v1/pods/{}", pod_name))
        .ok_or_else(|| {
            ErrResponse::new(
                String::from("Pod not found"),
                Some(format!("Pod {} not found", pod_name)),
            )
        })?;
    if pod.spec.containers.len() > 1 {
        Err(ErrResponse::new(
            String::from("Pod has more than one container"),
            None,
        ))
    } else {
        let uid = pod
            .metadata
            .uid
            .ok_or_else(|| ErrResponse::new(String::from("Pod has no uid"), None))?;
        let container = Container::new(Pod::unique_container_name(
            &uid,
            &pod_name,
            &pod.spec.containers[0].name,
        ));
        let logs = container.logs(&query.tail).await.map_err(|err| {
            ErrResponse::new(
                String::from("Failed to get container logs"),
                Some(err.to_string()),
            )
        })?;
        Ok(Json(Response::new(None, Some(logs))))
    }
}

#[derive(Serialize, Deserialize)]
pub struct LogsQuery {
    pub tail: Option<String>,
}
