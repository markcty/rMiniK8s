use std::sync::Arc;

use axum::{extract::Path, http::Uri, Extension, Json};
use axum_macros::debug_handler;
use resources::objects::KubeObject;

use super::{
    response::{HandlerResult, Response},
    utils::etcd_put,
};
use crate::AppState;

#[debug_handler]
pub async fn apply(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(pod_name): Path<String>,
    Json(payload): Json<KubeObject>,
    uri: Uri,
) -> HandlerResult<()> {
    // TODO: validate payload and add status
    etcd_put(app_state, uri.to_string(), payload).await?;
    let res = Response::new(format!("pod/{} created", pod_name), None);
    Ok(Json(res))
}
