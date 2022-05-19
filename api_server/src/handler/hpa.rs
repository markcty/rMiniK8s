use std::sync::Arc;

use axum::{
    extract::{Path, WebSocketUpgrade},
    response::IntoResponse,
    Extension, Json,
};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{hpa::HorizontalPodAutoscalerStatus, KubeObject, KubeResource},
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
    if let KubeResource::HorizontalPodAutoscaler(ref mut hpa) = payload.resource {
        let hpa_name = &payload.metadata.name;
        let result = etcd_get_object(
            &app_state,
            format!("/api/v1/horizontalpodautoscalers/{}", hpa_name),
            Some("horizontalpodautoscaler"),
        )
        .await;
        if result.is_ok() {
            return Err(ErrResponse::new(
                String::from("Error creating horizontal pod autoscaler"),
                Some(format!(
                    "Horizontal pod autoscaler {} already exists",
                    hpa_name
                )),
            ));
        }
        payload.metadata.uid = Some(Uuid::new_v4());
        let target_kind = hpa.spec.scale_target_ref.kind.to_lowercase();
        let target = etcd_get_object(
            &app_state,
            format!(
                "/api/v1/{}s/{}",
                target_kind, hpa.spec.scale_target_ref.name
            ),
            Some(&target_kind),
        )
        .await;
        match target {
            Ok(target) => match target.resource {
                KubeResource::ReplicaSet(rs) => {
                    let rs_status = rs.status.unwrap();
                    hpa.status = Some(HorizontalPodAutoscalerStatus {
                        current_replicas: rs_status.replicas,
                        desired_replicas: rs_status.replicas,
                        last_scale_time: None,
                    });
                    etcd_put(
                        &app_state,
                        format!("/api/v1/horizontalpodautoscalers/{}", hpa_name),
                        &payload,
                    )
                    .await?;
                    let res = Response::new(
                        Some(format!("horizontalpodautoscaler/{} created", hpa_name)),
                        None,
                    );
                    Ok(Json(res))
                },
                _ => Err(ErrResponse::new(
                    String::from("Error creating horizontal pod autoscaler"),
                    Some(format!(
                        "Target {} is not scalable",
                        hpa.spec.scale_target_ref.name
                    )),
                )),
            },
            Err(_) => Err(ErrResponse::new(
                String::from("Error creating horizontal pod autoscaler"),
                Some(format!(
                    "Scale target {} not found",
                    hpa.spec.scale_target_ref.name
                )),
            )),
        }
    } else {
        // TODO: fill business logic and error handling
        return Err(ErrResponse::new(
            String::from("Error creating horizontalpodautoscaler"),
            Some(format!(
                "Expecting horizontal pod autoscaler kind, got {}",
                payload.kind()
            )),
        ));
    }
}

#[debug_handler]
pub async fn get(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(hpa_name): Path<String>,
) -> HandlerResult<KubeObject> {
    let hpa = etcd_get_object(
        &app_state,
        format!("/api/v1/horizontalpodautoscalers/{}", hpa_name),
        Some("horizontalpodautoscaler"),
    )
    .await?;
    let res = Response::new(None, Some(hpa));
    Ok(Json(res))
}

#[debug_handler]
pub async fn delete(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(hpa_name): Path<String>,
) -> HandlerResult<()> {
    etcd_delete(
        &app_state,
        format!("/api/v1/horizontalpodautoscalers/{}", hpa_name),
    )
    .await?;
    let res = Response::new(
        Some(format!("horizontalpodautoscaler/{} deleted", hpa_name)),
        None,
    );
    Ok(Json(res))
}

#[debug_handler]
pub async fn update(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(hpa_name): Path<String>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // Ensure object exists, otherwise deleted object will be created:
    // Controller started processing -> User deleted object -> Controller update status
    etcd_get_object(
        &app_state,
        format!("/api/v1/horizontalpodautoscalers/{}", hpa_name),
        Some("horizontalpodautoscaler"),
    )
    .await?;
    if payload.kind() == "horizontalpodautoscaler" {
        etcd_put(
            &app_state,
            format!("/api/v1/horizontalpodautoscalers/{}", hpa_name),
            &payload,
        )
        .await?;
        let res = Response::new(
            Some(format!("horizontalpodautoscaler/{} updated", hpa_name)),
            None,
        );
        Ok(Json(res))
    } else {
        Err(ErrResponse::new(
            String::from("Error updating horizontal pod autoscaler"),
            Some(format!(
                "Expecting horizontal pod autoscaler kind, got {}",
                payload.kind()
            )),
        ))
    }
}

#[debug_handler]
pub async fn patch(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(hpa_name): Path<String>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    let mut object = etcd_get_object(
        &app_state,
        format!("/api/v1/horizontalpodautoscalers/{}", hpa_name),
        Some("horizontalpodautoscaler"),
    )
    .await?;
    match (&payload.resource, &mut object.resource) {
        (
            KubeResource::HorizontalPodAutoscaler(payload_hpa),
            KubeResource::HorizontalPodAutoscaler(ref mut hpa),
        ) => {
            hpa.spec = payload_hpa.spec.to_owned();
            etcd_put(
                &app_state,
                format!("/api/v1/horizontalpodautoscalers/{}", hpa_name),
                object,
            )
            .await?;
            let res = Response::new(
                Some(format!("horizontalpodautoscaler/{} patched", hpa_name)),
                None,
            );
            Ok(Json(res))
        },
        _ => Err(ErrResponse::new(
            String::from("Error patching horizontal pod autoscaler"),
            Some(format!(
                "Expecting horizontal pod autoscaler kind, got {}",
                payload.kind()
            )),
        )),
    }
}

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
) -> HandlerResult<Vec<KubeObject>> {
    let hpas = etcd_get_objects_by_prefix(
        &app_state,
        "/api/v1/horizontalpodautoscalers".to_string(),
        Some("horizontalpodautoscaler"),
    )
    .await?;

    let res = Response::new(None, Some(hpas));
    Ok(Json(res))
}

#[debug_handler]
pub async fn watch_all(
    Extension(app_state): Extension<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ErrResponse> {
    // open etcd watch connection
    let (watcher, stream) = etcd_watch_uri(&app_state, "/api/v1/horizontalpodautoscalers").await?;

    Ok(ws.on_upgrade(|socket| async move {
        forward_watch_to_ws::<KubeObject>(socket, watcher, stream).await
    }))
}
