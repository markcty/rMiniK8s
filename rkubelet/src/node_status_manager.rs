use std::{collections::HashMap, env, fs::File, io::Read};

use anyhow::Result;
use chrono::{Duration, Local, NaiveDateTime};
use interfaces::Interface;
use resources::{
    models::Response,
    objects::{
        node::{Capacity, Node, NodeAddressType, NodeInfo, NodeStatus},
        KubeObject, Metadata, Object,
    },
};
use sysinfo::{RefreshKind, System, SystemExt};
use tokio::time::sleep;

use crate::config::CONFIG;

pub struct NodeStatusManager {
    metadata: Metadata,
    status: NodeStatus,
    last_report: NaiveDateTime,
}

impl NodeStatusManager {
    pub fn new() -> Self {
        let system = System::new();
        let host_name = if let Some(host_name) = system.host_name() {
            host_name
        } else {
            panic!("Failed to get host name");
        };
        Self {
            metadata: Metadata {
                name: host_name,
                ..Default::default()
            },
            status: NodeStatus::default(),
            last_report: NaiveDateTime::from_timestamp(0, 0),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("Node status manager started");
        self.register_node().await;
        loop {
            sleep(std::time::Duration::from_secs(
                CONFIG.node_status_update_frequency,
            ))
            .await;
            let status = self.get_status();
            let now = Local::now().naive_utc();
            if status != self.status
                || now - self.last_report
                    > Duration::seconds(CONFIG.node_status_report_frequency as i64)
            {
                self.status = status;
                match self.post_status().await {
                    Ok(_) => {
                        tracing::info!("Posted node status");
                    },
                    Err(err) => {
                        tracing::error!("Failed to post node status: {:#}", err);
                    },
                }
            }
        }
    }

    async fn register_node(&mut self) {
        let client = reqwest::Client::new();
        self.status = self.get_status();
        let payload = self.object();
        const MAX_BACKOFF: u64 = 20;
        let mut backoff = 2;

        loop {
            let response = client
                .put(format!(
                    "{}{}",
                    CONFIG.cluster.api_server_url,
                    payload.uri()
                ))
                .json(&payload)
                .send()
                .await;
            match response {
                Ok(response) => {
                    let result = response.json::<Response<Node>>().await;
                    match result {
                        Ok(result) => {
                            // Node metadata is not persisted locally,
                            // so we need to fetch it during registration
                            if let Some(node) = result.data {
                                self.metadata = node.metadata;
                                tracing::info!("Node registered");
                                break;
                            }
                        },
                        Err(err) => {
                            tracing::error!("Failed to register node: {:#}", err);
                        },
                    }
                },
                Err(err) => tracing::error!("Failed to register node: {:#}", err),
            }
            sleep(std::time::Duration::from_secs(backoff.min(MAX_BACKOFF))).await;
            backoff += 2;
        }
    }

    fn get_status(&self) -> NodeStatus {
        let (allocatable, capacity) = self.get_capacity();
        NodeStatus {
            addresses: self.get_addresses(),
            allocatable,
            capacity,
            kubelet_port: CONFIG.port,
            last_heartbeat: Local::now().naive_utc(),
            node_info: self.get_info(),
        }
    }

    fn get_addresses(&self) -> HashMap<NodeAddressType, String> {
        let mut addresses = HashMap::new();
        let system = System::new();
        if let Some(host_name) = system.host_name() {
            addresses.insert(NodeAddressType::Hostname, host_name);
        }
        match Interface::get_all() {
            Ok(interfaces) => {
                interfaces
                    .iter()
                    .filter(|i| !i.is_loopback() && i.is_up() && i.is_running())
                    .for_each(|i| {
                        i.addresses.iter().for_each(|addr| {
                            if let Some(addr) = addr.addr {
                                tracing::debug!("{:#?}", i);
                                let ip = addr.ip();
                                if ip.is_ipv4() && i.name.starts_with("en") {
                                    addresses.insert(NodeAddressType::InternalIP, ip.to_string());
                                }
                            }
                        })
                    });
            },
            Err(err) => {
                tracing::error!("Failed to get interfaces: {:#}", err);
            },
        }
        addresses
    }

    fn get_info(&self) -> NodeInfo {
        let system = System::new();
        let mut machine_id = String::new();
        File::open("/etc/machine-id")
            .and_then(|mut f| f.read_to_string(&mut machine_id))
            .unwrap_or_default();
        // Remove "\n"
        machine_id.pop();

        NodeInfo {
            architecture: env::consts::ARCH.to_string(),
            machine_id,
            operating_system: env::consts::OS.to_string(),
            os_image: system.long_os_version().unwrap_or_default(),
        }
    }

    /// Return allocatable resources and total capacity
    fn get_capacity(&self) -> (Capacity, Capacity) {
        let system = System::new_with_specifics(RefreshKind::new().with_cpu().with_memory());
        let total = Capacity {
            cpu: system.processors().len() as u16,
            memory: system.total_memory(),
        };
        let allocatable = Capacity {
            cpu: system.processors().len() as u16,
            memory: system.available_memory(),
        };
        (allocatable, total)
    }

    fn object(&self) -> KubeObject {
        KubeObject::Node(Node {
            metadata: self.metadata.clone(),
            status: self.status.clone(),
        })
    }

    async fn post_status(&self) -> Result<()> {
        let client = reqwest::Client::new();
        let payload = self.object();
        client
            .put(format!(
                "{}{}",
                CONFIG.cluster.api_server_url,
                payload.uri()
            ))
            .json(&payload)
            .send()
            .await?
            .json::<Response<Node>>()
            .await?;
        Ok(())
    }
}
