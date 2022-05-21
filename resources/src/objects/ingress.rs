use serde::{Deserialize, Serialize};

use super::{Metadata, Object};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Ingress {
    pub metadata: Metadata,
    pub spec: IngressSpec,
}

impl Object for Ingress {
    fn kind(&self) -> &'static str {
        "ingress"
    }

    fn kind_plural(&self) -> String {
        "ingresses".to_string()
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct IngressSpec {
    /// A list of host rules used to configure the Ingress.
    pub rules: Vec<IngressRule>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct IngressRule {
    /// Host is the fully qualified domain name of a network host.
    /// It should always end with .minik8s.com
    /// Host is automatically generated if not specified
    pub host: Option<String>,
    /// A collection of paths that map requests to services.
    pub paths: Vec<IngressPath>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct IngressPath {
    /// Path is matched against the path of an incoming request.
    pub path: String,
    /// Service references a Service as a Backend.
    pub service: IngressService,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct IngressService {
    /// Name is the referenced service. The service must exist in the same namespace as the Ingress object.
    pub name: String,
    /// Port of the referenced service. A port number is required for a IngressServiceBackend.
    pub port: u16,
}
