use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ObjectReference {
    /// Kind of the referent.
    pub kind: String,
    /// Name of the referent.
    pub name: String,
}

impl ObjectReference {
    pub fn new(kind: String, name: String) -> ObjectReference {
        ObjectReference {
            kind,
            name,
        }
    }
}
