use resources::objects::{object_reference::ObjectReference, KubeObject};

pub fn dummy(_: &KubeObject, _: &Vec<KubeObject>) -> ObjectReference {
    ObjectReference {
        kind: "node".to_string(),
        name: "localhost".to_string(),
    }
}
