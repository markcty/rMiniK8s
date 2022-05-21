use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{Metadata, Object};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Node {
    pub metadata: Metadata,
    pub status: NodeStatus,
}

impl Object for Node {
    fn kind(&self) -> &'static str {
        "Node"
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeStatus {
    /// List of addresses reachable to the node.
    pub addresses: HashMap<NodeAddressType, String>,
    /// Endpoint on which Kubelet is listening.
    pub kubelet_port: u16,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub enum NodeAddressType {
    Hostname,
    ExternalIP,
    InternalIP,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct NodeInfo {
    /// MachineID reported by the node.
    /// For unique machine identification in the cluster this field is preferred.
    #[serde(rename = "machineID")]
    pub machine_id: String,
}
