use std::sync::Arc;

use dashmap::DashMap;
use resources::objects::KubeObject;

pub struct Cache {
    pub pod_cache: Arc<DashMap<String, KubeObject>>,
    pub node_cache: Arc<DashMap<String, KubeObject>>,
}

impl Cache {
    pub fn new(
        pod_cache: Arc<DashMap<String, KubeObject>>,
        node_cache: Arc<DashMap<String, KubeObject>>,
    ) -> Cache {
        Cache {
            pod_cache,
            node_cache,
        }
    }
}
