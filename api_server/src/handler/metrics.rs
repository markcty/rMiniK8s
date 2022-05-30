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
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::fs;

use super::response::HandlerResult;
use crate::{AppState, METRICS_SERVER_CONFIG};

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

#[debug_handler]
pub async fn add_target(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(payload): Json<HashMap<String, String>>,
) -> HandlerResult<()> {
    match (payload.get("job"), payload.get("target")) {
        (Some(job), Some(target)) => {
            add_scrape_target(job, target.to_owned(), &app_state.config.metrics_server).await?;
            Ok(Json(Response::new(Some("target added".to_string()), None)))
        },
        _ => Err(ErrResponse::new(
            String::from("Failed to add target"),
            Some(String::from("Missing job or target")),
        )),
    }
}

#[debug_handler]
pub async fn remove_target(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(payload): Json<HashMap<String, String>>,
) -> HandlerResult<()> {
    match (payload.get("job"), payload.get("target")) {
        (Some(job), Some(target)) => {
            remove_scrape_target(job, target.to_owned(), &app_state.config.metrics_server).await?;
            Ok(Json(Response::new(
                Some("target removed".to_string()),
                None,
            )))
        },
        _ => Err(ErrResponse::new(
            String::from("Failed to remove target"),
            Some(String::from("Missing job or target")),
        )),
    }
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

pub async fn read_config() -> Result<PromConfig, ErrResponse> {
    let file = fs::read_to_string(METRICS_SERVER_CONFIG)
        .await
        .map_err(|err| {
            tracing::error!("Failed to read metrics server config file: {}", err);
            ErrResponse::new(
                "Failed to read metrics server config file".to_string(),
                None,
            )
        })?;
    let config: PromConfig = serde_yaml::from_str(&file).map_err(|err| {
        tracing::error!("Failed to parse metrics server config file: {}", err);
        ErrResponse::new(
            "Failed to parse metrics server config file".to_string(),
            None,
        )
    })?;
    Ok(config)
}

pub async fn write_config(config: PromConfig) -> Result<(), ErrResponse> {
    let contents = serde_yaml::to_string(&config).map_err(|err| {
        tracing::error!("Failed to serialize metrics server config file: {}", err);
        ErrResponse::new(
            "Failed to serialize metrics server config".to_string(),
            None,
        )
    })?;
    fs::write(METRICS_SERVER_CONFIG, contents)
        .await
        .map_err(|err| {
            tracing::error!("Failed to write metrics server config file: {}", err);
            ErrResponse::new("Failed to write metrics server config".to_string(), None)
        })
}

pub async fn reload_config(metrics_server: &str) -> Result<(), ErrResponse> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/-/reload", metrics_server).as_str())
        .send()
        .await
        .map_err(|err| {
            ErrResponse::new(
                String::from("Failed to reload metrics server config"),
                Some(err.to_string()),
            )
        })?;
    if let Ok(text) = response.text().await {
        if !text.is_empty() {
            tracing::error!("{}", text);
            return Err(ErrResponse::new(
                String::from("Failed to reload metrics server config"),
                Some(text),
            ));
        }
    }
    Ok(())
}

/// Add a scrape target to a job in Prometheus, then reload the config.
pub async fn add_scrape_target(
    job_name: &str,
    target: String,
    metrics_server: &str,
) -> Result<(), ErrResponse> {
    let mut config = read_config().await?;
    // Add target
    if let Some(config) = config
        .scrape_configs
        .iter_mut()
        .find(|config| config.job_name == job_name)
    {
        // Job exists
        if !config.static_configs[0]
            .targets
            .iter()
            .any(|t| *t == target)
        {
            // Target doesn't exist
            config.static_configs[0].targets.push(target);
        }
    } else {
        // Job does not exist
        config.scrape_configs.push(PromScrapeConfig {
            job_name: job_name.to_string(),
            scrape_interval: Some("15s".to_string()),
            static_configs: vec![PromStaticConfig {
                targets: vec![target],
            }],
        });
    }
    write_config(config).await?;
    reload_config(metrics_server).await
}

/// Remove a scrape target from a job in Prometheus, then reload the config.
pub async fn remove_scrape_target(
    job_name: &str,
    target: String,
    metrics_server: &str,
) -> Result<(), ErrResponse> {
    let mut config = read_config().await?;
    // Remove target
    if let Some(config) = config
        .scrape_configs
        .iter_mut()
        .find(|config| config.job_name == job_name)
    {
        if let Some(index) = config.static_configs[0]
            .targets
            .iter()
            .position(|t| *t == target)
        {
            config.static_configs[0].targets.remove(index);
        }
    }
    write_config(config).await?;
    reload_config(metrics_server).await
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub selector: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct PromGlobalConfig {
    pub scrape_interval: Option<String>,
    pub external_labels: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
pub struct PromStaticConfig {
    pub targets: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct PromScrapeConfig {
    pub job_name: String,
    pub scrape_interval: Option<String>,
    pub static_configs: Vec<PromStaticConfig>,
}

#[derive(Serialize, Deserialize)]
pub struct PromConfig {
    pub global: PromGlobalConfig,
    pub scrape_configs: Vec<PromScrapeConfig>,
}
