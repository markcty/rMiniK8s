use serde::{Deserialize, Serialize};

use super::object_reference::ObjectReference;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Binding {
    target: ObjectReference,
}
