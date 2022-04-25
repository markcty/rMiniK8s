use std::sync::Arc;

use axum::{Extension, Json};
use axum_macros::debug_handler;

use super::{
    response::{HandlerResult, Response},
    utils,
};
use crate::{etcd::kv_to_str, AppState};

#[debug_handler]
pub async fn list(Extension(app_state): Extension<Arc<AppState>>) -> HandlerResult<Vec<String>> {
    let etcd_res = utils::get_prefix(&app_state, "/api/v1/nodes".to_string()).await?;
    // TODO: deserialize the value to Node KubeObjects
    let mut nodes: Vec<String> = Vec::new();
    for kv in etcd_res.kvs() {
        let (_, val_str) = kv_to_str(kv)?;
        nodes.push(val_str.to_string());
    }
    let res = Response::new("get nodes successfully".to_string(), Some(nodes));
    Ok(Json(res))
}
