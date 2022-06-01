use resources::{
    informer::Store,
    objects::{node::Node, pod::Pod},
};

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
