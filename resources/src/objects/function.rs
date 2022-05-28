use std::default::Default;

use serde::{Deserialize, Serialize};

use super::{
    hpa::{HorizontalPodAutoscalerBehavior, MetricSource},
    Labels, Metadata, Object,
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Function {
    pub metadata: Metadata,
    pub spec: FunctionSpec,
    pub status: Option<FunctionStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSpec {
    /// The upper limit for the number of replicas
    /// to which the autoscaler can scale up.
    #[serde(default = "default_max_replicas")]
    pub max_replicas: u32,
    /// Configures the scaling behavior of the target
    /// in both Up and Down directions
    /// (scaleUp and scaleDown fields respectively).
    /// If not set, the default HPAScalingRules
    /// for scale up and scale down are used.
    #[serde(default)]
    pub behavior: HorizontalPodAutoscalerBehavior,
    /// Contains the specifications for which to use
    /// to calculate the desired replica count
    /// (the maximum replica count across all metrics will be used).
    /// The desired replica count is calculated multiplying the ratio
    /// between the target value and the current value
    /// by the current number of pods.
    /// Ergo, metrics used must decrease as the pod count is increased, and vice-versa.
    pub metrics: MetricSource,
}

fn default_max_replicas() -> u32 {
    10
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FunctionStatus {
    pub service_ref: String,
    pub filename: String,
    pub host: String,
    /// the name of image which wraps this function
    pub image: Option<String>,
}

impl Object for Function {
    fn kind(&self) -> &'static str {
        "Function"
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}

impl Function {
    pub fn init(&mut self, svc_name: String, filename: String) {
        let name = &self.metadata.name;
        let host = format!("{}.func.minik8s.com", name);
        let mut labels = Labels::new();
        labels.insert("function", name);
        self.metadata.uid = Some(uuid::Uuid::new_v4());
        self.metadata.labels = labels;
        self.status = Some(FunctionStatus {
            service_ref: svc_name,
            filename,
            host,
            image: None,
        });
    }
}
