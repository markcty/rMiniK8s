use resources::{informer::Store, objects::KubeObject};

pub struct Cache {
    pub pod_cache: Store<KubeObject>,
    pub node_cache: Store<KubeObject>,
}

impl Cache {
    pub fn new(pod_cache: Store<KubeObject>, node_cache: Store<KubeObject>) -> Cache {
        Cache {
            pod_cache,
            node_cache,
        }
    }
}
