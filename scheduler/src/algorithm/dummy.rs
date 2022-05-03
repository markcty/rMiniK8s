use resources::objects::{object_reference::ObjectReference, KubeObject};

use crate::cache::Cache;

pub fn dummy(_: &KubeObject, _: &Cache) -> ObjectReference {
    ObjectReference {
        kind: "node".to_string(),
        name: "localhost".to_string(),
    }
}
