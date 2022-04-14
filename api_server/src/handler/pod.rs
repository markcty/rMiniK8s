use axum::extract::Path;
use axum::http::Uri;
use axum::{Extension, Json};
use axum_macros::debug_handler;

use resources::objects::KubeObject;

use crate::handler::{ErrResponse, HandlerResult, Response};
use crate::{etcd, AppState};

#[debug_handler]
pub async fn apply(
    Extension(app_state): Extension<AppState>,
    Path(pod_name): Path<String>,
    Json(payload): Json<KubeObject>,
    uri: Uri,
) -> HandlerResult<()> {
    let mut client = app_state.get_client().await?;
    // TODO: validate payload and add status
    etcd::put(&mut client, uri.to_string(), payload)
        .await
        .map_err(ErrResponse::from)?;
    let res = Response::new(format!("pod/{} created", pod_name), None);
    Ok(Json(res))
}
