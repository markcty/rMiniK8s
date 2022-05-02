use std::collections::HashMap;

use resources::objects::{
    node::{Node, NodeAddressType, NodeStatus},
    KubeObject, Metadata,
};

pub fn dummy(_: &KubeObject) -> KubeObject {
    let mut addresses: HashMap<NodeAddressType, String> = HashMap::new();
    addresses.insert(NodeAddressType::Hostname, "localhost".to_string());

    KubeObject {
        metadata: Metadata {
            name: "local".to_string(),
            uid: None,
        },
        resource: resources::objects::KubeResource::Node(Node {
            status: NodeStatus {
                addresses,
                kubelet_port: 10250,
            },
        }),
    }
}
