use anyhow::Result;
use reqwest::Client;
use resources::{
    models::Response,
    objects::{
        metrics::{PodMetric, PodMetrics, PodMetricsInfo, Resource},
        Labels,
    },
};

use crate::CONFIG;

pub struct MetricsClient {
    client: Client,
}

impl MetricsClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Get pod metrics in raw value
    pub async fn get_resource_metric_value(
        &self,
        resource: &Resource,
        selector: &Labels,
    ) -> Result<PodMetricsInfo> {
        let metrics = self.get_pod_metrics(selector).await?;
        if metrics.is_empty() {
            Err(anyhow::anyhow!("No metrics found"))
        } else {
            let mut metric_info = PodMetricsInfo::new();
            for pod in metrics {
                let mut sum = 0;
                if pod.containers.is_empty() {
                    continue;
                }
                for container in pod.containers {
                    let usage = container.usage.get(resource);
                    match usage {
                        Some(usage) => sum += *usage,
                        None => {
                            tracing::debug!(
                                "Missing resource metric {} for container {} in pod {}",
                                resource,
                                container.name,
                                pod.name
                            );
                            break;
                        },
                    }
                }
                metric_info.insert(
                    pod.name,
                    PodMetric {
                        timestamp: pod.timestamp,
                        window: pod.window,
                        value: sum,
                    },
                );
            }
            Ok(metric_info)
        }
    }

    async fn get_pod_metrics(&self, selector: &Labels) -> Result<Vec<PodMetrics>> {
        let response = self
            .client
            .get(format!("{}/api/v1/metrics/pods", CONFIG.api_server_url,))
            .query::<Vec<(&str, String)>>(&vec![("selector", selector.to_string())])
            .send()
            .await?
            .json::<Response<Vec<PodMetrics>>>()
            .await?;
        match response.data {
            Some(data) => Ok(data),
            None => Err(anyhow::anyhow!("Failed to get pod metrics")),
        }
    }
}
