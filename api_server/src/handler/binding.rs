use std::sync::Arc;

use axum::{Extension, Json};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{
        node::NodeAddressType,
        pod::{PodCondition, PodConditionType},
        KubeObject, Object,
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
    if let KubeObject::Binding(binding) = &payload {
        // get node
        let object = etcd_get_object(
            &app_state,
            format!("/api/v1/nodes/{}", binding.target.name),
            Some("node"),
        )
        .await?;
        let node = match object {
            KubeObject::Node(node) => node,
            _ => {
                return Err(ErrResponse::new(
                    String::from("bind error"),
                    Some(format!(
                        "Expecting bind target to be node kind, got {}",
                        object.kind()
                    )),
                ));
            },
        };

        // get pod object
        let mut object: KubeObject = etcd_get_object(
            &app_state,
            format!("/api/v1/pods/{}", binding.metadata.name),
            Some("pod"),
        )
        .await?;
        // update pod
        if let KubeObject::Pod(ref mut pod) = object {
            let mut status = &mut pod.status.as_mut().expect("Pod should have a status");
            status.conditions.insert(
                PodConditionType::PodScheduled,
                PodCondition {
                    status: true,
                },
            );
            status.host_ip = node
                .status
                .addresses
                .get(&NodeAddressType::InternalIP)
                .map(|addr| addr.to_owned());
            pod.spec.node_name = Some(binding.target.name.clone());
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
        etcd_put(&app_state, &object).await?;
        etcd_put(&app_state, &payload).await?;
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
