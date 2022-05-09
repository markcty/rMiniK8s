use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ObjectReference {
    /// Kind of the referent.
    pub kind: String,
    /// Name of the referent.
    pub name: String,
}
