use std::collections::HashMap;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Serialize, Deserialize, Hash, Clone, Eq, PartialEq, Display)]
pub enum Resource {
    CPU,
    Memory,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
/// Metrics of containers in a pod.
pub struct PodMetrics {
    /// Pod name
    pub name: String,
    pub timestamp: NaiveDateTime,
    /// Duration in seconds over which the metrics were gathered.
    pub window: u32,
    /// Metrics for all containers collected within the same time window.
    pub containers: Vec<ContainerMetrics>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ContainerMetrics {
    pub name: String,
    pub usage: HashMap<Resource, i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
/// An overall summary of PodMetrics.
pub struct PodMetric {
    pub timestamp: NaiveDateTime,
    /// Duration in seconds over which the metrics were gathered.
    pub window: u32,
    pub value: i64,
}

/// A mapping from pod names to metrics.
pub type PodMetricsInfo = HashMap<String, PodMetric>;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FunctionMetric {
    /// Function name
    pub name: String,
    pub timestamp: NaiveDateTime,
    pub value: i64,
}
