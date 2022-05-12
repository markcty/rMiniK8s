pub mod kubelet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct ClusterConfig {
    /// API server URL
    pub api_server_url: String,
    /// API server watch URL
    pub api_server_watch_url: String,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        ClusterConfig {
            api_server_url: "http://localhost:8080".to_string(),
            api_server_watch_url: "ws://localhost:8080".to_string(),
        }
    }
}
