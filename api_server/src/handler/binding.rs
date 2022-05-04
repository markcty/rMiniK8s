use std::sync::Arc;

use axum::{Extension, Json};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{
        pod::{PodCondition, PodConditionType},
        KubeObject, KubeResource,
    },
};

use crate::{
    handler::{
        response::HandlerResult,
        utils::{etcd_get_object, etcd_put},
    },
    AppState,
};

#[debug_handler]
#[allow(dead_code)]
pub async fn bind(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // check payload
    if let KubeResource::Binding(binding) = &payload.resource {
        let pod_name = payload.name();
        // get pod object
        let mut object: KubeObject =
            etcd_get_object(&app_state, format!("/api/v1/pods/{}", pod_name), None).await?;
        // update pod
        if let KubeResource::Pod(ref mut pod) = object.resource {
            let mut status = pod.status.clone().unwrap_or_default();
            status.conditions.insert(
                PodConditionType::PodScheduled,
                PodCondition {
                    status: true,
                },
            );
            // TODO: get the node, and get the address of that node, then set the host_ip according to that.
            status.host_ip = Some("127.0.0.1".to_string());
            pod.status = Some(status);

            let mut spec = pod.spec.clone();
            spec.node_name = Some(binding.target.name.clone());
            pod.spec = spec;
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
        etcd_put(&app_state, format!("/api/v1/pods/{}", pod_name), &object).await?;
        etcd_put(
            &app_state,
            format!("/api/v1/bindings/{}", pod_name),
            &payload,
        )
        .await?;
        let res = Response::new(Some("bind successfully".to_string()), None);
        Ok(Json(res))
    } else {
        let res = ErrResponse::new(
            "bind error".to_string(),
            Some(format!(
                "the kind of payload: {}, which is not binding",
                payload.kind()
            )),
        );
        Err(res)
    }
}
