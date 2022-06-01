use std::{str::FromStr, sync::Arc};

use axum::{
    extract::{Multipart, Path, WebSocketUpgrade},
    http::Request,
    response::IntoResponse,
    Extension, Json,
};
use axum_macros::debug_handler;
use hyper::{Body, Uri};
use resources::{
    models::{ErrResponse, Response},
    objects::{
        gpu_job::GpuJobStatus, object_reference::ObjectReference, pod::Pod, KubeObject, Object,
    },
};
use uuid::Uuid;

use super::{response::HandlerResult, utils::*};
use crate::{etcd::forward_watch_to_ws, AppState};

#[debug_handler]
pub async fn create(
    Extension(app_state): Extension<Arc<AppState>>,
    mut multipart: Multipart,
) -> HandlerResult<()> {
    let mut gpu_job: Option<KubeObject> = None;
    let mut filename: Option<String> = None;
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().map_or("".to_string(), |n| n.to_owned());
        match name.as_str() {
            "gpujob" => {
                gpu_job = Some(decode_field_json(field).await?);
            },
            "code" => {
                filename = Some(store_code_file(field).await?);
            },
            _ => {},
        }
    }

    let mut gpu_job = gpu_job
        .ok_or_else(|| ErrResponse::bad_request("Job field is not presented".to_string(), None))?;
    let filename = filename
        .ok_or_else(|| ErrResponse::bad_request("Code field is not presented".to_string(), None))?;

    if let KubeObject::GpuJob(ref mut job) = gpu_job {
        let job_name = &job.metadata.name.to_owned();
        let result = etcd_get_object(&app_state, job.uri(), Some("gpujob")).await;
        if result.is_ok() {
            return Err(ErrResponse::new(
                String::from("Error creating gpu job"),
                Some(format!("Gpu job {} already exists", job_name)),
            ));
        }
        job.metadata.uid = Some(Uuid::new_v4());
        let status = GpuJobStatus {
            filename: Some(filename),
            ..Default::default()
        };
        job.status = Some(status);

        etcd_put(&app_state, &gpu_job).await?;
        let res = Response::new(Some(format!("gpujob/{} created", job_name)), None);
        Ok(Json(res))
    } else {
        return Err(ErrResponse::new(
            String::from("Error creating gpujob"),
            Some(format!("Expecting gpujob kind, got {}", gpu_job.kind())),
        ));
    }
}

#[debug_handler]
pub async fn get(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(job_name): Path<String>,
) -> HandlerResult<KubeObject> {
    let job = etcd_get_object(
        &app_state,
        format!("/api/v1/gpujobs/{}", job_name),
        Some("gpujob"),
    )
    .await?;
    let res = Response::new(None, Some(job));
    Ok(Json(res))
}

#[debug_handler]
pub async fn delete(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(job_name): Path<String>,
) -> HandlerResult<()> {
    etcd_delete(&app_state, format!("/api/v1/gpujobs/{}", job_name)).await?;
    let res = Response::new(Some(format!("gpujob/{} deleted", job_name)), None);
    Ok(Json(res))
}

#[debug_handler]
pub async fn update(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(job_name): Path<String>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    if let KubeObject::GpuJob(_) = payload {
        etcd_put(&app_state, &payload).await?;
        let res = Response::new(Some(format!("gpujob/{} updated", job_name)), None);
        Ok(Json(res))
    } else {
        Err(ErrResponse::new(
            String::from("Error updating gpujob"),
            Some(format!("Expecting gpujob kind, got {}", payload.kind())),
        ))
    }
}

#[debug_handler]
pub async fn patch(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(job_name): Path<String>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    let mut object = etcd_get_object(
        &app_state,
        format!("/api/v1/gpujobs/{}", job_name),
        Some("gpujob"),
    )
    .await?;
    match (&payload, &mut object) {
        (KubeObject::GpuJob(payload_job), KubeObject::GpuJob(ref mut job)) => {
            job.spec = payload_job.spec.to_owned();
            etcd_put(&app_state, &object).await?;
            let res = Response::new(Some(format!("gpujob/{} patched", job_name)), None);
            Ok(Json(res))
        },
        _ => Err(ErrResponse::new(
            String::from("Error patching gpujob"),
            Some(format!("Expecting gpujob kind, got {}", payload.kind())),
        )),
    }
}

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
) -> HandlerResult<Vec<KubeObject>> {
    let gpujobs =
        etcd_get_objects_by_prefix(&app_state, "/api/v1/gpujobs".to_string(), Some("gpujob"))
            .await?;

    let res = Response::new(None, Some(gpujobs));
    Ok(Json(res))
}

#[debug_handler]
pub async fn watch_all(
    Extension(app_state): Extension<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ErrResponse> {
    // open etcd watch connection
    let (watcher, stream) = etcd_watch_uri(&app_state, "/api/v1/gpujobs").await?;

    Ok(ws.on_upgrade(|socket| async move {
        forward_watch_to_ws::<KubeObject>(socket, watcher, stream).await
    }))
}

#[debug_handler]
pub async fn job_logs(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(job_name): Path<String>,
    request: Request<Body>,
) -> axum::http::Response<Body> {
    let job = etcd_get_object(
        &app_state,
        format!("/api/v1/gpujobs/{}", job_name),
        Some("gpujob"),
    )
    .await;
    if job.is_err() {
        return axum::http::Response::new(Body::from(
            ErrResponse::new(format!("Job {} dosen't exist", job_name), None).json(),
        ));
    }

    let pods = etcd_get_objects_by_prefix(&app_state, "/api/v1/pods".to_string(), Some("pod"))
        .await
        .map_err(|e| axum::http::Response::new(Body::from(e.json())));

    let pods = match pods {
        Ok(objects) => {
            let mut pods: Vec<Pod> = Vec::new();
            for pod in objects {
                if let KubeObject::Pod(pod) = pod {
                    pods.push(pod);
                }
            }
            pods.iter()
                .filter(|pod| {
                    pod.is_succeeded()
                        && pod
                            .metadata
                            .owner_references
                            .contains(&ObjectReference::new("GpuJob".to_owned(), job_name.clone()))
                })
                .map(|pod| pod.to_owned())
                .collect::<Vec<_>>()
        },
        _ => {
            return axum::http::Response::new(Body::from(
                ErrResponse::new(String::from("Pods not found"), None).json(),
            ))
        },
    };

    let pod_name = match pods.first() {
        Some(pod) => pod.name().to_owned(),
        _ => {
            return axum::http::Response::new(Body::from(
                ErrResponse::new(format!("Job {} is not completed", job_name), None).json(),
            ))
        },
    };
    let pod_uri = Uri::from_str(format!("/api/v1/pods/{}/logs", pod_name).as_str()).unwrap();
    proxy_to_rkubelet(app_state, pod_uri, pod_name, request).await
}
