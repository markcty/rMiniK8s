use serde::{Deserialize, Serialize};

pub mod pod;

#[derive(Debug, Serialize, Deserialize)]
pub struct KubeObject {
    pub kind: String,
    pub metadata: Metadata,
    pub spec: KubeSpec,
    status: Option<KubeStatus>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KubeSpec {
    Pod(pod::PodSpec),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KubeStatus {
    Pod(pod::PodStatus),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
}
