use std::collections::HashMap;

use chrono::NaiveDateTime;
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
    /// Allocatable represents the resources of a node
    /// that are available for scheduling.
    /// Defaults to Capacity.
    pub allocatable: Capacity,
    /// Capacity represents the total resources of a node.
    pub capacity: Capacity,
    /// Endpoint on which Kubelet is listening.
    pub kubelet_port: u16,
    /// Set of ids/uuids to uniquely identify the node.
    pub node_info: NodeInfo,
    /// Last heartbeat time, set by rKubelet.
    pub last_heartbeat: NaiveDateTime,
}

impl Default for NodeStatus {
    fn default() -> Self {
        NodeStatus {
            addresses: HashMap::new(),
            allocatable: Capacity::default(),
            capacity: Capacity::default(),
            kubelet_port: 10250,
            node_info: NodeInfo::default(),
            last_heartbeat: NaiveDateTime::from_timestamp(0, 0),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub enum NodeAddressType {
    Hostname,
    ExternalIP,
    InternalIP,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct NodeInfo {
    /// The Architecture reported by the node. Required
    pub architecture: String,
    /// MachineID reported by the node. Required
    /// For unique machine identification in the cluster this field is preferred.
    #[serde(rename = "machineID")]
    pub machine_id: String,
    /// The Operating System reported by the node
    pub operating_system: String,
    /// OS Image reported by the node from /etc/os-release
    pub os_image: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct Capacity {
    /// Number of cpu cores on the node.
    pub cpu: u16,
    /// Amount of memory in kilobytes.
    pub memory: u64,
}
