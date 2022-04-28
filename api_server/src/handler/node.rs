use std::sync::Arc;

use axum::{Extension, Json};
use axum_macros::debug_handler;
use resources::objects::KubeObject;

use super::{
    response::{HandlerResult, Response},
    utils::etcd_get_objects_by_prefix,
};
use crate::AppState;

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
) -> HandlerResult<Vec<KubeObject>> {
    let nodes =
        etcd_get_objects_by_prefix(&app_state, "/api/v1/nodes".to_string(), Some("node")).await?;

    let res = Response::new(None, Some(nodes));
    Ok(Json(res))
}
