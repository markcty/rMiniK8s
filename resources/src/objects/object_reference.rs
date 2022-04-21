use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ObjectReference {
    /// Kind of the referent.
    kind: String,
    /// Name of the referent.
    name: String,
}
