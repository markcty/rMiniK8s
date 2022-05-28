use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use super::{
    function::Function, metrics::Resource, object_reference::ObjectReference, Metadata, Object,
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HorizontalPodAutoscaler {
    pub metadata: Metadata,
    pub spec: HorizontalPodAutoscalerSpec,
    pub status: Option<HorizontalPodAutoscalerStatus>,
}

impl Object for HorizontalPodAutoscaler {
    fn kind(&self) -> &'static str {
        "HorizontalPodAutoscaler"
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}

impl HorizontalPodAutoscaler {
    pub fn from_function(func: &Function) -> Self {
        let func_name = func.metadata.name.to_owned();
        let metadata = Metadata {
            name: func_name.to_owned(),
            uid: None,
            labels: func.metadata.labels.clone(),
            owner_references: vec![ObjectReference {
                kind: "function".to_string(),
                name: func_name.to_owned(),
            }],
        };
        let spec = HorizontalPodAutoscalerSpec {
            scale_target_ref: ObjectReference {
                kind: "ReplicaSet".to_string(),
                name: func_name,
            },
            behavior: func.spec.behavior.to_owned(),
            min_replicas: 0,
            max_replicas: func.spec.max_replicas,
            metrics: func.spec.metrics.to_owned(),
        };
        Self {
            metadata,
            spec,
            status: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HorizontalPodAutoscalerSpec {
    /// The upper limit for the number of replicas
    /// to which the autoscaler can scale up.
    /// It cannot be less that minReplicas.
    pub max_replicas: u32,
    /// The lower limit for the number of replicas
    /// to which the autoscaler can scale down.
    /// It defaults to 1 pod.
    #[serde(default = "default_min_replicas")]
    pub min_replicas: u32,
    /// Points to the target resource to scale,
    /// and is used to the pods for which metrics should be collected,
    /// as well as to actually change the replica count.
    pub scale_target_ref: ObjectReference,
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
    /// If not set, the default metric will be set to 80% average CPU utilization.
    #[serde(default = "default_metrics")]
    pub metrics: MetricSource,
}

fn default_min_replicas() -> u32 {
    1
}

fn default_metrics() -> MetricSource {
    MetricSource::Resource(ResourceMetricSource::default())
}

/// HorizontalPodAutoscalerBehavior configures the scaling behavior
/// of the target in both Up and Down directions
/// (scaleUp and scaleDown fields respectively).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HorizontalPodAutoscalerBehavior {
    /// Scaling policy for scaling Down.
    /// If not set, the default value is to allow to scale down
    /// to minReplicas pods, with a 60 second stabilization window
    /// (i.e., the highest recommendation for the last 60sec is used).
    #[serde(default = "default_scale_down_behavior")]
    pub scale_down: HPAScalingRules,
    /// Scaling policy for scaling Up.
    /// If not set, the default value is the higher of:
    /// - increase no more than 4 pods per 60 seconds
    /// - double the number of pods per 60 seconds
    /// No stabilization is used.
    #[serde(default = "default_scale_up_behavior")]
    pub scale_up: HPAScalingRules,
}

fn default_scale_down_behavior() -> HPAScalingRules {
    HPAScalingRules {
        policies: vec![HPAScalingPolicy {
            type_: ScalingPolicyType::Percent,
            value: 100,
            period_seconds: 60,
        }],
        select_policy: PolicySelection::Max,
        stabilization_window_seconds: 60,
    }
}

fn default_scale_up_behavior() -> HPAScalingRules {
    HPAScalingRules {
        policies: vec![
            HPAScalingPolicy {
                type_: ScalingPolicyType::Pods,
                value: 4,
                period_seconds: 60,
            },
            HPAScalingPolicy {
                type_: ScalingPolicyType::Percent,
                value: 100,
                period_seconds: 60,
            },
        ],
        select_policy: PolicySelection::Max,
        stabilization_window_seconds: 0,
    }
}

impl Default for HorizontalPodAutoscalerBehavior {
    fn default() -> Self {
        HorizontalPodAutoscalerBehavior {
            scale_down: default_scale_down_behavior(),
            scale_up: default_scale_up_behavior(),
        }
    }
}

/// HPAScalingRules configures the scaling behavior for one direction.
/// These Rules are applied after calculating DesiredReplicas
/// from metrics for the HPA.
/// They can limit the scaling velocity by specifying scaling policies.
/// They can prevent flapping by specifying the stabilization window,
/// so that the number of replicas is not set instantly,
/// instead, the safest value from the stabilization window is chosen.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HPAScalingRules {
    /// A list of potential scaling polices which can be used during scaling.
    /// At least one policy must be specified,
    /// otherwise the HPAScalingRules will be discarded as invalid.
    pub policies: Vec<HPAScalingPolicy>,
    /// Specify which policy should be used.
    /// If not set, the default value Max is used.
    #[serde(default)]
    pub select_policy: PolicySelection,
    /// Number of seconds for which past recommendations should be considered
    /// while scaling up or scaling down.
    /// Must be greater than or equal to zero and less than or equal to 3600 (one hour).
    /// If not set, use the default values:
    /// - For scale up: 0 (i.e. no stabilization is done).
    /// - For scale down: 300 (i.e. the stabilization window is 300 seconds long).
    pub stabilization_window_seconds: u32,
}

impl HPAScalingRules {
    pub fn longest_period(&self) -> u32 {
        self.policies
            .iter()
            .map(|policy| policy.period_seconds)
            .max()
            .unwrap_or(0)
    }
}

/// PolicySelection describes how to choose a policy from multiple ones
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum PolicySelection {
    /// Select the policy with the lowest recommendation value.
    Min,
    /// Select the policy with the highest recommendation value.
    Max,
    /// Disable current action
    Disabled,
}

impl Default for PolicySelection {
    fn default() -> Self {
        PolicySelection::Max
    }
}

/// HPAScalingPolicy is a single policy
/// which must hold true for a specified past interval.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HPAScalingPolicy {
    /// Specify the scaling policy.
    #[serde(rename = "type")]
    pub type_: ScalingPolicyType,
    /// Contains the amount of change which is permitted by the policy.
    /// It must be greater than zero.
    pub value: u32,
    /// Specifies the window of time for which the policy should hold true.
    /// PeriodSeconds must be greater than zero
    /// and less than or equal to 1800(30 min).
    pub period_seconds: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ScalingPolicyType {
    Pods,
    Percent,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum MetricSource {
    Resource(ResourceMetricSource),
    Function(FunctionMetricSource),
}

/// ResourceMetricSource indicates how to scale on a resource metric
/// known to Kubernetes, as specified in requests and limits,
/// describing each pod in the current scale target (e.g. CPU or memory).
/// The values will be averaged together before being compared to the target.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ResourceMetricSource {
    /// Name of the resource.
    pub name: Resource,
    /// Target value for the given metric
    pub target: MetricTarget,
}

impl Default for ResourceMetricSource {
    fn default() -> Self {
        ResourceMetricSource {
            name: Resource::CPU,
            target: MetricTarget::AverageUtilization(80),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FunctionMetricSource {
    /// Name of the function.
    pub name: String,
    /// Target average queries in a 15s window.
    pub target: u64,
}

/// MetricTarget defines the target value, average value,
/// or average utilization of a specific metric.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MetricTarget {
    /// Target value of the average of the resource metric
    /// across all relevant pods,
    /// represented as a percentage of the requested value
    /// of the resource for the pods.
    AverageUtilization(u32),
    /// Target value of the average of the resource metric
    /// across all relevant pods.
    /// Percentage for CPU, megabytes for memory.
    AverageValue(u64),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HorizontalPodAutoscalerStatus {
    /// Desired number of replicas of pods managed by this autoscaler,
    /// as last calculated by the autoscaler.
    pub desired_replicas: u32,
    /// Current number of replicas of pods managed by this autoscaler,
    /// as last seen by the autoscaler.
    pub current_replicas: u32,
    /// Last time the HorizontalPodAutoscaler scaled the number of pods,
    /// used by the autoscaler to control how often the number of pods is changed.
    pub last_scale_time: Option<NaiveDateTime>,
}
