use dashmap::{mapref::one::Ref, DashMap};

use crate::pod::Pod;

#[allow(dead_code)]
pub struct PodManager {
    pods: DashMap<String, Pod>,
}

#[allow(dead_code)]
impl PodManager {
    pub fn new() -> Self {
        Self {
            pods: DashMap::new(),
        }
    }

    pub fn add_pod(&mut self, pod: Pod) {
        self.pods.insert(pod.metadata().name.to_owned(), pod);
    }

    pub fn get_pod(&self, name: &str) -> Option<Ref<String, Pod>> {
        self.pods.get(name)
    }

    pub fn remove_pod(&mut self, name: &str) {
        self.pods.remove(name);
    }

    pub fn iter_mut(&self) -> dashmap::iter::IterMut<String, Pod> {
        self.pods.iter_mut()
    }
}
