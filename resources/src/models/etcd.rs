use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum WatchEvent {
    Put(PutEvent),
    Delete,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PutEvent {
    key: String,
    value: String,
}

impl WatchEvent {
    pub fn new_put(key: String, value: String) -> Self {
        WatchEvent::Put(PutEvent {
            key,
            value,
        })
    }

    pub fn new_delete() -> Self {
        WatchEvent::Delete
    }
}
