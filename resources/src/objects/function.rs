use std::default::Default;

use serde::{Deserialize, Serialize};

use super::{Metadata, Object};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Function {
    pub metadata: Metadata,
    pub spec: FunctionSpec,
    pub status: FunctionStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSpec {
    pub service_ref: String,
    pub filename: String,
    pub host: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FunctionStatus {
    /// the name of image which wraps this function
    pub image: Option<String>,
    /// where the function is ready for the client to request
    pub ready: bool,
}

impl Object for Function {
    fn kind(&self) -> &'static str {
        "Function"
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}

impl Function {
    pub fn new(name: String, svc_name: String, filename: String) -> Self {
        let host = format!("{}.func.minik8s.com", name);
        let metadata = Metadata {
            name,
            uid: Some(uuid::Uuid::new_v4()),
            labels: super::Labels::new(),
            owner_references: Vec::new(),
        };
        let spec = FunctionSpec {
            filename,
            host,
            service_ref: svc_name,
        };
        let status = FunctionStatus {
            image: None,
            ready: false,
        };
        Self {
            metadata,
            spec,
            status,
        }
    }
}
