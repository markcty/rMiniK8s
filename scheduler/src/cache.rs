use std::collections::HashMap;

use resources::{
    informer::Store,
    objects::{node::Node, pod::Pod, Labels},
};

#[derive(Debug, Default)]
pub struct NodeState {
    pub name: String,
    pub labels: Labels,
    pub is_ready: bool,
    pub pod_count: u32,
}

pub struct Cache {
    pub pod_cache: Store<Pod>,
    pub node_cache: Store<Node>,
    pub node_states: HashMap<String, NodeState>,
}

impl Cache {
    /// Initialize a new cache,
    /// node states should be manually refreshed before using.
    pub fn new(pod_cache: Store<Pod>, node_cache: Store<Node>) -> Cache {
        Cache {
            pod_cache,
            node_cache,
            node_states: HashMap::new(),
        }
    }

    /// Re-calculate node states.
    pub async fn refresh(&mut self) {
        self.node_states.clear();
        let store = self.node_cache.read().await;
        for node in store.values() {
            let state = self.calculate_node_state(node).await;
            self.node_states
                .insert(node.metadata.name.to_owned(), state);
        }
    }

    pub async fn handle_pod_add(&mut self, pod: Pod, node_name: &String) {
        match self.node_states.get_mut(node_name) {
            Some(node_state) => {
                node_state.pod_count += 1;
            },
            None => {
                tracing::warn!(
                    "Pod {} is scheduled to node {} which is not in the node cache",
                    pod.metadata.name,
                    node_name
                );
            },
        }
    }

    pub async fn handle_pod_delete(&mut self, pod: Pod) {
        if let Some(node_name) = &pod.spec.node_name {
            match self.node_states.get_mut(node_name) {
                Some(node_state) => {
                    node_state.pod_count -= 1;
                },
                None => {
                    tracing::warn!(
                        "Pod {} is scheduled to node {} which is not in the node cache",
                        pod.metadata.name,
                        node_name
                    );
                },
            }
        }
    }

    pub async fn handle_node_add(&mut self, node: Node) {
        let is_ready = node.is_ready();
        self.node_states.insert(
            node.metadata.name.to_owned(),
            NodeState {
                name: node.metadata.name,
                labels: node.metadata.labels,
                is_ready,
                ..Default::default()
            },
        );
    }

    pub async fn handle_node_update(&mut self, new_node: Node) {
        let new_state = self.calculate_node_state(&new_node).await;
        self.node_states.insert(new_node.metadata.name, new_state);
    }

    pub async fn handle_node_delete(&mut self, node: Node) {
        self.node_states.remove(&node.metadata.name);
    }

    pub async fn calculate_node_state(&self, node: &Node) -> NodeState {
        let mut node_state = NodeState {
            name: node.metadata.name.to_owned(),
            labels: node.metadata.labels.to_owned(),
            is_ready: node.is_ready(),
            ..Default::default()
        };
        let pods = self.pod_cache.read().await;
        for pod in pods.values() {
            if pod.spec.node_name == Some(node.metadata.name.to_owned()) {
                node_state.pod_count += 1;
            }
        }
        node_state
    }
}
