use std::sync::Arc;

use axum::{extract::Path, Extension, Json};
use axum_macros::debug_handler;
use resources::objects::{
    pod::{PodCondition, PodConditionType},
    KubeObject, KubeResource,
};

use super::{
    response::{ErrResponse, HandlerResult, Response},
    utils::*,
};
use crate::AppState;

#[debug_handler]
#[allow(dead_code)]
pub async fn bind(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(pod_name): Path<String>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // check payload
    if payload.kind() != "binding" {
        let res = ErrResponse::new(
            "bind error".to_string(),
            Some(format!(
                "the kind of payload: {}, which is not binding",
                payload.kind()
            )),
        );
        return Err(res);
    }
    // get pod object
    let mut object: KubeObject =
        etcd_get_object(&app_state, format!("/api/v1/pods/{}", pod_name), None).await?;
    // update pod
    // Because we have only one type in KubeResource now,
    // there will be a warning temporarily.
    #[allow(irrefutable_let_patterns)]
    if let KubeResource::Pod(ref mut pod) = object.resource {
        let mut status = pod.status.clone().unwrap_or_default();
        status.conditions.insert(
            PodConditionType::PodScheduled,
            PodCondition {
                status: true,
            },
        );
        pod.status = Some(status);
    } else {
        tracing::error!("object kind error");
        return Err(ErrResponse::new(
            String::from("bind error"),
            Some(format!(
                "the kind of object: {}, which is not pod",
                object.kind()
            )),
        ));
    }
    // put it back
    etcd_put(&app_state, format!("/api/v1/pods/{}", pod_name), object).await?;
    etcd_put(
        &app_state,
        format!("/api/v1/bindings/{}", pod_name),
        payload.clone(),
    )
    .await?;
    let res = Response::new("bind successfully".to_string(), None);
    Ok(Json(res))
}
