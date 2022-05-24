use resources::objects::{object_reference::ObjectReference, pod::Pod};

use crate::cache::{Cache, NodeState};

pub fn simple(_pod: &Pod, cache: &Cache) -> Option<ObjectReference> {
    let mut candidate = None;
    cache
        .node_states
        .iter()
        .filter(|(_, state)| state.is_ready)
        .for_each(|(_, state)| {
            if is_better_than(Some(state), candidate) {
                candidate = Some(state);
            }
        });
    candidate.map(|state| ObjectReference {
        kind: "node".to_string(),
        name: state.name.to_owned(),
    })
}

/// Determine if node1 is a better candidate then node2
fn is_better_than(node1: Option<&NodeState>, node2: Option<&NodeState>) -> bool {
    if node1.is_none() {
        return false;
    }
    if node2.is_none() {
        return true;
    }
    node1.unwrap().pod_count < node2.unwrap().pod_count
}
