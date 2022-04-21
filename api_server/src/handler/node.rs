use std::sync::Arc;

use axum::{http::Uri, Extension, Json};
use axum_macros::debug_handler;

use super::{
    response::{HandlerResult, Response},
    utils::etcd_get_prefix,
};
use crate::{etcd::kv_to_str, AppState};

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
    uri: Uri,
) -> HandlerResult<Vec<String>> {
    // uri: /api/v1/nodes
    let etcd_res = etcd_get_prefix(&app_state, uri.to_string()).await?;
    let mut nodes: Vec<String> = Vec::new();
    for kv in etcd_res.kvs() {
        let (_, val_str) = kv_to_str(kv)?;
        nodes.push(val_str.to_string());
    }
    let res = Response::new("get nodes successfully".to_string(), Some(nodes));
    Ok(Json(res))
}
