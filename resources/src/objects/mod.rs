use serde::{Deserialize, Serialize};

pub mod pod;

#[derive(Debug, Serialize, Deserialize)]
pub struct KubeObject {
    pub kind: String,
    pub metadata: Metadata,
    spec: KubeSpec,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KubeSpec {
    Pod(pod::PodSpec),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
}
