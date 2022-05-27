use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::Local;
use resources::{
    informer::Store,
    objects::{
        metrics::{PodMetric, PodMetricsInfo, Resource},
        pod::{Pod, PodConditionType, PodPhase},
        Labels,
    },
};

use crate::metrics::MetricsClient;

pub struct ReplicaCalculator {
    client: MetricsClient,
    pod_store: Store<Pod>,
}

impl ReplicaCalculator {
    pub fn new(pod_store: Store<Pod>) -> Self {
        Self {
            client: MetricsClient::new(),
            pod_store,
        }
    }

    /// Calculate desired replica count based on target utilization
    pub async fn calc_replicas_by_utilization(
        &self,
        current_replicas: u32,
        target_utilization: u32,
        resource: &Resource,
        selector: &Labels,
    ) -> Result<u32> {
        let mut metrics = self
            .client
            .get_resource_metric_value(resource, selector)
            .await?;
        let pods = self.get_pods(selector).await;
        if pods.is_empty() {
            return Err(anyhow::anyhow!("No pods found when calculating replicas"));
        }

        let missing_pods = self.filter_pods(&mut metrics, &pods)?;
        let scale_ratio =
            self.calc_scale_ratio_by_utilization(&metrics, &pods, target_utilization, resource);
        // Make conservative assumption for missing pods
        for pod in missing_pods {
            metrics.insert(
                pod.metadata.name.to_owned(),
                PodMetric {
                    timestamp: Local::now().naive_utc(),
                    window: 60,
                    value: if scale_ratio < 1.0 {
                        // When scaling down, treat missing pods as 100% usage
                        pod.requests(resource)
                    } else {
                        // When scaling up, treat missing pods as 0% usage
                        0
                    },
                },
            );
        }
        let new_scale_ratio =
            self.calc_scale_ratio_by_utilization(&metrics, &pods, target_utilization, resource);
        let new_replicas = (new_scale_ratio * metrics.len() as f64).ceil() as u32;
        // Our assumption shouldn't change the scale direction,
        // and new scale ratio should cause a change in the scale direction
        if (new_scale_ratio > 1.0 && (scale_ratio < 1.0 || new_replicas < current_replicas))
            || (new_scale_ratio < 1.0 && (scale_ratio > 1.0 || new_replicas > current_replicas))
        {
            Ok(current_replicas)
        } else {
            Ok(new_replicas)
        }
    }

    /// Calculate desired replica count based on target value
    pub async fn calc_replicas_by_value(
        &self,
        current_replicas: u32,
        target_value: u64,
        resource: &Resource,
        selector: &Labels,
    ) -> Result<u32> {
        let mut metrics = self
            .client
            .get_resource_metric_value(resource, selector)
            .await?;
        let pods = self.get_pods(selector).await;
        if pods.is_empty() {
            return Err(anyhow::anyhow!("No pods found when calculating replicas"));
        }

        let missing_pods = self.filter_pods(&mut metrics, &pods)?;
        let scale_ratio = self.calc_scale_ratio_by_value(&metrics, target_value);
        // Make conservative assumption for missing pods
        for object in missing_pods {
            metrics.insert(
                object.metadata.name,
                PodMetric {
                    timestamp: Local::now().naive_utc(),
                    window: 60,
                    value: if scale_ratio < 1.0 {
                        // When scaling down, treat missing pods as 100% usage
                        target_value as i64
                    } else {
                        // When scaling up, treat missing pods as 0% usage
                        0
                    },
                },
            );
        }

        let new_scale_ratio = self.calc_scale_ratio_by_value(&metrics, target_value);
        let new_replicas = (new_scale_ratio * metrics.len() as f64).ceil() as u32;
        // Our assumption shouldn't change the scale direction,
        // and new scale ratio should cause a change in the scale direction
        if (new_scale_ratio > 1.0 && (scale_ratio < 1.0 || new_replicas < current_replicas))
            || (new_scale_ratio < 1.0 && (scale_ratio > 1.0 || new_replicas > current_replicas))
        {
            Ok(current_replicas)
        } else {
            Ok(new_replicas)
        }
    }

    /// Calculate desired replica count based on function QPS metrics
    pub async fn calc_function_replicas(
        &self,
        current_replicas: u32,
        target_value: u64,
        func_name: &str,
    ) -> Result<u32> {
        let metric = self.client.get_function_metric(func_name).await?;
        let current_value = metric.value / current_replicas as i64;
        tracing::info!(
            "Calculating function replicas: current_value={}, current_total={}",
            current_value,
            metric.value
        );
        let scale_ratio = current_value as f64 / target_value as f64;
        Ok((current_replicas as f64 * scale_ratio).ceil() as u32)
    }

    /// Filter out failed and invalid pods
    fn filter_pods(&self, metrics: &mut PodMetricsInfo, pods: &[Pod]) -> Result<Vec<Pod>> {
        // Pods that haven't been present in the metrics yet
        let mut missing_pods = Vec::<Pod>::new();

        for pod in pods {
            let status = pod
                .status
                .as_ref()
                .with_context(|| format!("Missing status for pod {}", pod.metadata.name))?;
            let metric = metrics.get(&pod.metadata.name);
            if metric.is_none() {
                tracing::debug!("No metrics found for pod {}", pod.metadata.name);
                missing_pods.push(pod.to_owned());
                continue;
            }
            if status.phase == PodPhase::Failed
                || !status
                    .conditions
                    .get(&PodConditionType::Ready)
                    .map(|c| c.status)
                    .unwrap_or(false)
            {
                tracing::info!("Ignored Pod {} since it's not ready", pod.metadata.name);
                metrics.remove(&pod.metadata.name);
                continue;
            }
        }
        Ok(missing_pods)
    }

    fn calc_scale_ratio_by_utilization(
        &self,
        metrics: &HashMap<String, PodMetric>,
        pods: &[Pod],
        target_utilization: u32,
        resource: &Resource,
    ) -> f64 {
        let metrics_total: i64 = metrics.iter().map(|(_, m)| m.value).sum();
        let requests_total = pods
            .iter()
            .filter(|p| metrics.contains_key(&p.metadata.name))
            .map(|p| p.requests(resource))
            .sum::<i64>();
        if requests_total == 0 {
            return 1.0;
        }

        tracing::info!(
            "Calculating replicas by utilization: metrics_total={}, requests_total={}",
            metrics_total,
            requests_total
        );
        // Calculate utilization in percentage
        let current_utilization = (metrics_total * 100) as f64 / requests_total as f64;
        current_utilization / target_utilization as f64
    }

    fn calc_scale_ratio_by_value(
        &self,
        metrics: &HashMap<String, PodMetric>,
        target_value: u64,
    ) -> f64 {
        let metrics_total: i64 = metrics.iter().map(|(_, m)| m.value).sum();
        // Current average value
        let current_value = metrics_total / metrics.len() as i64;
        if target_value == 0 {
            return 1.0;
        }
        tracing::info!(
            "Calculating replicas by value: current_value={}",
            current_value
        );
        current_value as f64 / target_value as f64
    }

    async fn get_pods(&self, selector: &Labels) -> Vec<Pod> {
        self.pod_store
            .read()
            .await
            .iter()
            .filter(|(_, pod)| pod.metadata.labels.matches(selector))
            .map(|(_, pod)| pod.to_owned())
            .collect::<Vec<_>>()
    }
}
