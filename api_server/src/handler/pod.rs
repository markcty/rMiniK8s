use std::sync::Arc;

use axum::{extract::Path, http::Uri, Extension, Json};
use axum_macros::debug_handler;
use resources::objects::{pod::PodPhase, KubeObject, KubeResource};

use super::{
    response::{ErrResponse, HandlerResult, Response},
    utils::{etcd_put, update_pod_phase},
};
use crate::AppState;

#[debug_handler]
pub async fn apply(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(pod_name): Path<String>,
    Json(mut payload): Json<KubeObject>,
    uri: Uri,
) -> HandlerResult<()> {
    // TODO: validate payload and add status
    // Because we have only one type in KubeResource now,
    // there will be a warning temporarily.
    #[allow(irrefutable_let_patterns)]
    if let KubeResource::Pod(ref mut pod) = payload.resource {
        update_pod_phase(pod, PodPhase::Pending);
        etcd_put(&app_state, uri.to_string(), payload.clone()).await?;
        let res = Response::new(format!("pod/{} created", pod_name), None);
        let queue = &mut app_state.schedule_queue.write().unwrap();
        queue.push(payload);
        Ok(Json(res))
    } else {
        // TODO: fill business logic and error handling
        tracing::debug!("payload kind error");
        return Err(ErrResponse::new(
            String::from("apply error"),
            Some(format!(
                "the kind of payload: {}, which is not pod",
                payload.kind()
            )),
        ));
    }
}
