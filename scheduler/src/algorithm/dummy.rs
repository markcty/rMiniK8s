use resources::objects::{object_reference::ObjectReference, pod::Pod};

use crate::cache::Cache;

#[allow(dead_code)]
pub fn dummy(_: &Pod, _: &Cache) -> Option<ObjectReference> {
    Some(ObjectReference {
        kind: "node".to_string(),
        name: "localhost".to_string(),
    })
}
