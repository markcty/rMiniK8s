use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum WatchEvent {
    Put(PutEvent),
    Delete(DeleteEvent),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PutEvent {
    pub key: String,
    pub object: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeleteEvent {
    pub key: String,
}

impl WatchEvent {
    pub fn new_put(key: String, object: String) -> Self {
        WatchEvent::Put(PutEvent {
            key,
            object,
        })
    }

    pub fn new_delete(key: String) -> Self {
        WatchEvent::Delete(DeleteEvent {
            key,
        })
    }
}
