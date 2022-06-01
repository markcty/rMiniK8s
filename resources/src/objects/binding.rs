use serde::{Deserialize, Serialize};

use super::{object_reference::ObjectReference, Metadata, Object};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Binding {
    pub metadata: Metadata,
    pub target: ObjectReference,
}

impl Object for Binding {
    fn kind(&self) -> &'static str {
        "Binding"
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}
