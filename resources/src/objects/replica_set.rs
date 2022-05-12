use serde::{Deserialize, Serialize};

use super::{pod::PodTemplateSpec, Labels};

/// ReplicaSet ensures that a specified number of pod replicas are running
/// at any given time.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReplicaSet {
    /// Defines the specification of the desired behavior of the ReplicaSet.
    pub spec: ReplicaSetSpec,
    /// The most recently observed status of the ReplicaSet.
    /// This data may be out of date by some window of time.
    /// Populated by the system. Read-only.
    pub status: Option<ReplicaSetStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReplicaSetSpec {
    /// A label query over pods that should match the replica count.
    /// Label keys and values that must match
    /// in order to be controlled by this replica set.
    /// It must match the pod template's labels.
    /// Required.
    pub selector: Labels,
    /// The object that describes the pod
    /// that will be created if insufficient replicas are detected.
    pub template: PodTemplateSpec,
    /// The number of desired replicas.
    /// This is a pointer to distinguish between explicit zero and unspecified.
    /// Defaults to 1.
    #[serde(default = "default_replicas")]
    pub replicas: u32,
}

fn default_replicas() -> u32 {
    1
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSetStatus {
    /// The most recently oberved number of replicas.
    pub replicas: u32,
    /// The number of pods targeted by this ReplicaSet with a Ready Condition.
    pub ready_replicas: u32,
}
