use resources::{informer::Store, objects::{pod::Pod, node::Node}};

pub struct Cache {
    pub pod_cache: Store<Pod>,
    pub node_cache: Store<Node>,
}

impl Cache {
    pub fn new(pod_cache: Store<Pod>, node_cache: Store<Node>) -> Cache {
        Cache {
            pod_cache,
            node_cache,
        }
    }
}
