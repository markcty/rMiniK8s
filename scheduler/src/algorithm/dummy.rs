use resources::objects::{object_reference::ObjectReference, pod::Pod};

use crate::cache::Cache;

pub fn dummy(_: &Pod, _: &Cache) -> ObjectReference {
    ObjectReference {
        kind: "node".to_string(),
        name: "localhost".to_string(),
    }
}
