use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    net::Ipv4Addr,
};

use serde::{Deserialize, Serialize};

use super::{object_reference::ObjectReference, Metadata, Object};
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

impl Service {
    pub fn from_function(name: &str, func_name: &str, cluster_ip: Ipv4Addr) -> Self {
        let metadata = Metadata {
            name: name.to_owned(),
            uid: Some(uuid::Uuid::new_v4()),
            labels: Labels::default(),
            owner_references: vec![ObjectReference {
                kind: "function".to_string(),
                name: func_name.to_string(),
            }],
        };
        let spec = ServiceSpec {
            selector: Labels(HashMap::from([("func".to_string(), func_name.to_string())])),
            ports: vec![ServicePort {
                port: 80,
                target_port: 80,
            }],
            endpoints: HashSet::new(),
            cluster_ip: Some(cluster_ip),
        };
        Self {
            metadata,
            spec,
        }
    }
}
