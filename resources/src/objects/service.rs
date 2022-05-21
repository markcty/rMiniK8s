use std::{collections::HashSet, fmt::Debug, net::Ipv4Addr};

use serde::{Deserialize, Serialize};

use super::{Metadata, Object};
use crate::objects::Labels;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Service {
    pub metadata: Metadata,
    pub spec: ServiceSpec,
}

impl Object for Service {
    fn kind(&self) -> &'static str {
        "Service"
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSpec {
    /// Route service traffic to pods with label keys and values matching this selector.
    pub selector: Labels,
    /// The list of ports that are exposed by this service.
    pub ports: Vec<ServicePort>,
    /// a collection of endpoints that implement the actual service
    #[serde(default)]
    pub endpoints: HashSet<Ipv4Addr>,
    /// clusterIP is the IP address of the service and is usually assigned randomly
    pub cluster_ip: Option<Ipv4Addr>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServicePort {
    /// The port that will be exposed by this service.
    pub port: u16,
    /// Number of the port to access on the pods targeted by the service.
    pub target_port: u16,
}
