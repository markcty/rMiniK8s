use serde::{Deserialize, Serialize};

use crate::objects::Object;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum WatchEvent<T> {
    Put(PutEvent<T>),
    Delete(DeleteEvent),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PutEvent<T> {
    pub key: String,
    pub object: T,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeleteEvent {
    pub key: String,
}

impl<T: Object> WatchEvent<T> {
    pub fn new_put(key: String, object: T) -> Self {
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
