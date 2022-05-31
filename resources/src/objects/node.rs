use std::{collections::HashMap, fmt::Write};

use chrono::{Local, NaiveDateTime, TimeZone};
use indenter::indented;
use serde::{Deserialize, Serialize};
use strum::Display;

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

impl Node {
    pub fn internal_ip(&self) -> Option<String> {
        self.status
            .addresses
            .iter()
            .find(|(type_, _)| NodeAddressType::InternalIP.eq(type_))
            .map(|(_, ip)| ip.to_owned())
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:<16} {}", "Name:", self.metadata.name)?;
        writeln!(f, "{:<16} {}", "Labels:", self.metadata.labels.to_string())?;
        let status = &self.status;
        writeln!(
            f,
            "{:<16} {}",
            "Last Heartbeat:",
            Local.from_utc_datetime(&status.last_heartbeat)
        )?;

        writeln!(f, "Addresses:")?;
        for (address_type, address) in status.addresses.iter() {
            writeln!(indented(f), "{}: {}", address_type, address)?;
        }

        writeln!(f, "Capacity:")?;
        write!(indented(f), "{}", status.capacity)?;
        writeln!(f, "Allocatable:")?;
        write!(indented(f), "{}", status.allocatable)?;

        writeln!(f, "System Info:")?;
        write!(indented(f), "{}", status.node_info)
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

#[derive(Debug, Serialize, Deserialize, Display, PartialEq, Eq, Hash, Clone)]
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

impl std::fmt::Display for NodeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:<20} {}", "Architecture:", self.architecture)?;
        writeln!(f, "{:<20} {}", "Machine ID:", self.machine_id)?;
        writeln!(f, "{:<20} {}", "Operating System:", self.operating_system)?;
        writeln!(f, "{:<20} {}", "OS Image:", self.os_image)
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct Capacity {
    /// Number of cpu cores on the node.
    pub cpu: u16,
    /// Amount of memory in kilobytes.
    pub memory: u64,
}

impl std::fmt::Display for Capacity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "CPU: {}", self.cpu)?;
        writeln!(f, "Memory: {}KB", self.memory)
    }
}
