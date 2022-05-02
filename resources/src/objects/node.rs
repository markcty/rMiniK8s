use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Node {
    pub status: NodeStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeInfo {
    /// MachineID reported by the node.
    /// For unique machine identification in the cluster this field is preferred.
    #[serde(rename = "machineID")]
    pub machine_id: String,
}
