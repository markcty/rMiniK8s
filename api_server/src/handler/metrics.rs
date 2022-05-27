use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query},
    Extension, Json,
};
use axum_macros::debug_handler;
use chrono::Local;
use prometheus_http_api::{
    DataSourceBuilder, DataSourceError, InstantQuery, Query as PromQuery, Response as PromResponse,
    ResponseData, VectorResult,
};
use resources::{
    models::{ErrResponse, Response},
    objects::{
        metrics::{ContainerMetrics, FunctionMetric, PodMetrics, Resource},
        Labels,
    },
};
use serde::Deserialize;
use serde_json::Value as JsonValue;

use super::response::HandlerResult;
use crate::AppState;

#[debug_handler]
pub async fn list_pods(
    Extension(app_state): Extension<Arc<AppState>>,
    query: Query<ListQuery>,
) -> HandlerResult<Vec<PodMetrics>> {
    let selector = match &query.selector {
        Some(selector) => {
            let selector = Labels::try_from(selector);
            if selector.is_err() {
                return Err(ErrResponse::new(
                    String::from("Failed to get metrics"),
                    Some(String::from("Failed to parse selector")),
                ));
            }
            selector.unwrap()
        },
        None => Labels::new(),
    };
    let selector_str = get_selector_str(&selector);
    let mut pod_containers = HashMap::<String, HashMap<String, ContainerMetrics>>::new();

    // Get CPU metrics
    let query = PromQuery::Instant(InstantQuery::new(&format!(
        "sum(\
            rate(\
                container_cpu_usage_seconds_total{{\
                    {}\
                    container_label_minik8s_container_type=\"container\"\
                }}[1m]\
            )\
        ) by(container_label_minik8s_container_name, container_label_minik8s_pod_name)",
        selector_str
    )));
    let response = DataSourceBuilder::new(&app_state.config.metrics_server)
        .with_query(query)
        .build()
        .unwrap()
        .get()
        .await;
    let containers = unwrap_vector_result(response)?;
    for container in containers {
        if let Some(names) = get_names(&container.labels) {
            let (container_name, pod_name) = names;
            let (_, value) = unwrap_instant_value(container.value)?;
            pod_containers
                .entry(pod_name.to_owned())
                .or_insert_with(HashMap::new)
                .entry(container_name.to_owned())
                .or_insert(ContainerMetrics {
                    name: container_name.to_owned(),
                    usage: HashMap::new(),
                })
                .usage
                // Convert to milli-CPU
                .insert(Resource::CPU, (value * 1000.0) as i64);
        } else {
            continue;
        }
    }

    // Get memory metrics
    let query = PromQuery::Instant(InstantQuery::new(&format!(
        "container_memory_working_set_bytes{{\
            {}\
            container_label_minik8s_container_type=\"container\"\
        }}",
        selector_str
    )));
    let response = DataSourceBuilder::new(&app_state.config.metrics_server)
        .with_query(query)
        .build()
        .unwrap()
        .get()
        .await;
    let containers = unwrap_vector_result(response)?;
    for container in containers {
        if let Some(names) = get_names(&container.labels) {
            let (container_name, pod_name) = names;
            let (_, value) = unwrap_instant_value(container.value)?;
            pod_containers
                .entry(pod_name.to_owned())
                .or_insert_with(HashMap::new)
                .entry(container_name.to_owned())
                .or_insert(ContainerMetrics {
                    name: container_name.to_owned(),
                    usage: HashMap::new(),
                })
                .usage
                .insert(Resource::Memory, value as i64);
        } else {
            continue;
        }
    }

    // Aggregate metrics by pod
    let metrics = pod_containers
        .into_iter()
        .map(|(pod_name, containers)| PodMetrics {
            name: pod_name,
            containers: containers.into_values().collect(),
            timestamp: Local::now().naive_utc(),
            window: 60,
        })
        .collect::<Vec<_>>();
    Ok(Json(Response::new(None, Some(metrics))))
}

#[debug_handler]
pub async fn get_function(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(func_name): Path<String>,
) -> HandlerResult<FunctionMetric> {
    let query = PromQuery::Instant(InstantQuery::new(&format!(
        "idelta(\
            function_requests_total{{\
                function=\"{}\"\
            }}[1m]\
        )",
        func_name
    )));
    let response = DataSourceBuilder::new(&app_state.config.metrics_server)
        .with_query(query)
        .build()
        .unwrap()
        .get()
        .await;
    let metrics = unwrap_vector_result(response)?;
    if metrics.is_empty() {
        return Err(ErrResponse::new(
            String::from("Failed to get metrics"),
            Some(String::from("No metrics found")),
        ));
    }
    let value = unwrap_instant_value(metrics[0].to_owned().value)?;
    Ok(Json(Response::new(
        None,
        Some(FunctionMetric {
            name: func_name,
            timestamp: Local::now().naive_utc(),
            value: value.1 as i64,
        }),
    )))
}

fn unwrap_vector_result(
    response: Result<PromResponse, DataSourceError>,
) -> Result<Vec<VectorResult>, ErrResponse> {
    match response {
        Ok(response) => match response.data {
            ResponseData::Vector {
                result,
            } => Ok(result),
            _ => Err(ErrResponse::new(
                String::from("Failed to get metrics from metrics server"),
                Some(String::from("Expecting vector result")),
            )),
        },
        Err(err) => Err(ErrResponse::new(
            String::from("Failed to get metrics from metrics server"),
            Some(err.to_string()),
        )),
    }
}

fn get_names(labels: &HashMap<String, String>) -> Option<(&String, &String)> {
    let container_name = labels.get("container_label_minik8s_container_name");
    let pod_name = labels.get("container_label_minik8s_pod_name");
    match (container_name, pod_name) {
        (Some(container_name), Some(pod_name)) => Some((container_name, pod_name)),
        _ => None,
    }
}

fn unwrap_instant_value(value: Vec<JsonValue>) -> Result<(f64, f64), ErrResponse> {
    if value.len() != 2 {
        return Err(ErrResponse::new(
            String::from("Failed to parse metrics response"),
            Some(String::from("Expecting array of length 2")),
        ));
    }
    match (value[0].as_f64(), value[1].as_str()) {
        (Some(timestamp), Some(value)) => match value.parse::<f64>() {
            Ok(value) => Ok((timestamp, value)),
            _ => Err(ErrResponse::new(
                String::from("Failed to parse metrics response"),
                Some(String::from("Failed to parse value as f64")),
            )),
        },
        _ => Err(ErrResponse::new(
            String::from("Failed to parse metrics response"),
            Some(String::from("Expecting pair of f64 and string")),
        )),
    }
}

fn get_selector_str(selector: &Labels) -> String {
    if selector.0.is_empty() {
        return "".to_string();
    }
    selector
        .0
        .iter()
        .map(|(k, v)| format!("container_label_{}=\"{}\"", k, v))
        .collect::<Vec<_>>()
        .join(",")
        + ","
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub selector: Option<String>,
}
