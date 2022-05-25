use serde::{Deserialize, Serialize};

use super::{pod::PodTemplateSpec, Labels, Metadata, Object};

/// ReplicaSet ensures that a specified number of pod replicas are running
/// at any given time.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReplicaSet {
    pub metadata: Metadata,
    /// Defines the specification of the desired behavior of the ReplicaSet.
    pub spec: ReplicaSetSpec,
    /// The most recently observed status of the ReplicaSet.
    /// This data may be out of date by some window of time.
    /// Populated by the system. Read-only.
    pub status: Option<ReplicaSetStatus>,
}

impl Object for ReplicaSet {
    fn kind(&self) -> &'static str {
        "ReplicaSet"
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}

impl std::fmt::Display for ReplicaSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:<16} {}", "Name:", self.metadata.name)?;
        writeln!(f, "{:<16} {}", "Selector:", self.spec.selector.to_string())?;
        writeln!(f, "{:<16} {}", "Labels:", self.metadata.labels.to_string())?;
        if self.status.is_none() {
            return Ok(());
        }
        let status = self.status.as_ref().unwrap();
        writeln!(
            f,
            "{:<16} {} ready / {} current / {} desired",
            "Replicas:", status.ready_replicas, status.replicas, self.spec.replicas
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSetStatus {
    /// The most recently oberved number of replicas.
    pub replicas: u32,
    /// The number of pods targeted by this ReplicaSet with a Ready Condition.
    pub ready_replicas: u32,
}
