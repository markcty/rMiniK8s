use std::sync::Arc;

use anyhow::{anyhow, Result};
use axum::{extract::Path, Extension, Json};
use axum_macros::debug_handler;
use resources::{
    models::{ErrResponse, Response},
    objects::{
        workflow::{State, Workflow},
        KubeObject, Object,
    },
};
use uuid::Uuid;

use super::{response::HandlerResult, utils::*};
use crate::AppState;

#[debug_handler]
pub async fn create(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(mut payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // TODO: validate payload
    if let KubeObject::Workflow(ref mut workflow) = payload {
        workflow.metadata.uid = Some(Uuid::new_v4());

        validate_workflow(&app_state, workflow).await.map_err(|e| {
            tracing::info!("Error validating workflow, caused by: {}", e);
            ErrResponse::new(format!("Error validating workflow, caused by: {}", e), None)
        })?;

        etcd_put(&app_state, &payload).await?;
        let res = Response::new(Some(format!("workflow/{} created", payload.name())), None);
        Ok(Json(res))
    } else {
        // TODO: fill business logic and error handling
        return Err(ErrResponse::new(
            String::from("Error creating workflow"),
            Some(format!("Expecting workflow, got {}", payload.kind())),
        ));
    }
}

async fn validate_workflow(app_state: &Arc<AppState>, workflow: &Workflow) -> Result<()> {
    for (_, state) in workflow.spec.states.iter() {
        if let State::Task(task) = state {
            if etcd_get_object(
                app_state,
                format!("/api/v1/functions/{}", task.resource),
                Some("function"),
            )
            .await
            .is_err()
            {
                return Err(anyhow!(format!(
                    "Function {} does not exist",
                    task.resource
                )));
            };
            if let Some(ref next) = task.next {
                if !workflow.spec.states.contains_key(next.as_str()) {
                    return Err(anyhow!(format!("State {} does not exist", next)));
                }
            }
        }
        // TODO: other checkings
    }
    Ok(())
}

#[debug_handler]
pub async fn update(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(payload): Json<KubeObject>,
) -> HandlerResult<()> {
    // TODO: validate payload
    if let KubeObject::Workflow(ref workflow) = payload {
        validate_workflow(&app_state, workflow).await.map_err(|e| {
            tracing::info!("Error validating workflow, caused by: {}", e);
            ErrResponse::new(format!("Error validating workflow, caused by: {}", e), None)
        })?;
        etcd_put(&app_state, &payload).await?;
        let res = Response::new(Some(format!("workflow/{} updated", payload.name())), None);
        Ok(Json(res))
    } else {
        // TODO: fill business logic and error handling
        return Err(ErrResponse::new(
            String::from("Error creating workflow"),
            Some(format!("Expecting workflow, got {}", payload.kind())),
        ));
    }
}

#[debug_handler]
pub async fn get(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
) -> HandlerResult<KubeObject> {
    let workflow = etcd_get_object(
        &app_state,
        format!("/api/v1/workflows/{}", name),
        Some("workflow"),
    )
    .await?;
    let res = Response::new(None, Some(workflow));
    Ok(Json(res))
}

#[debug_handler]
pub async fn delete(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
) -> HandlerResult<()> {
    etcd_delete(&app_state, format!("/api/v1/workflows/{}", name)).await?;
    let res = Response::new(Some(format!("workflows/{} deleted", name)), None);
    Ok(Json(res))
}

#[debug_handler]
pub async fn list(
    Extension(app_state): Extension<Arc<AppState>>,
) -> HandlerResult<Vec<KubeObject>> {
    let workflows = etcd_get_objects_by_prefix(
        &app_state,
        "/api/v1/workflows".to_string(),
        Some("workflow"),
    )
    .await?;

    let res = Response::new(None, Some(workflows));
    Ok(Json(res))
}
