use serde::{Deserialize, Serialize};

use super::object_reference::ObjectReference;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Binding {
    pub target: ObjectReference,
}
