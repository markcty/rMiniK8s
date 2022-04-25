use std::sync::Arc;

use axum::{extract::Path, http::Uri, Extension, Json};
use axum_macros::debug_handler;
use resources::objects::{
    pod::{PodCondition, PodConditionType},
    KubeObject,
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
    uri: Uri,
) -> HandlerResult<()> {
    if payload.kind() != "binding" {
        let res = ErrResponse::new("object type error".to_string(), None);
        return Err(res);
    }
    // uri: /api/v1/bindings/:pod_name
    // get pod
    let mut pod = get_pod_from_etcd(&app_state, pod_name.clone()).await?;
    // update pod
    update_pod_condition(
        &mut pod,
        PodConditionType::PodScheduled,
        PodCondition {
            status: true,
        },
    );
    // put it back
    put_pod_to_etcd(&app_state, pod_name, pod).await?;
    // TODO: check whether payload is a Binding KubeObject
    etcd_put(&app_state, uri.to_string(), payload.clone()).await?;
    let res = Response::new("bind successfully".to_string(), None);
    Ok(Json(res))
}
