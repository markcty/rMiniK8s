use std::{collections::HashMap, default::Default};

use bollard::models::{ContainerInspectResponse, ContainerStateStatusEnum};
use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pod {
    pub spec: PodSpec,
    pub status: Option<PodStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PodSpec {
    /// List of containers belonging to the pod.
    /// Containers cannot currently be added or removed.
    /// There must be at least one container in a Pod. Cannot be updated.
    pub containers: Vec<Container>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Container {
    /// Name of the container specified as a DNS_LABEL.
    /// Each container in a pod must have a unique name (DNS_LABEL).
    /// Cannot be updated.
    pub name: String,
    /// Docker image name.
    pub image: String,
    /// List of ports to expose from the container.
    pub ports: Vec<ContainerPort>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContainerPort {
    /// Number of port to expose on the pod's IP address.
    /// This must be a valid port number, 0 < x < 65536.
    pub container_port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PodStatus {
    /// IP address of the host to which the pod is assigned.
    /// Empty if not yet scheduled.
    #[serde(rename = "hostIP")]
    pub host_ip: Option<String>,
    /// RFC 3339 date and time at which the object was acknowledged by the Kubelet.
    /// This is before the Kubelet pulled the container image(s) for the pod.
    pub start_time: NaiveDateTime,
    /// The phase of a Pod is a simple, high-level summary
    /// of where the Pod is in its lifecycle.
    pub phase: PodPhase,
    /// IP address allocated to the pod.
    /// Routable at least within the cluster.
    /// Empty if not yet allocated.
    #[serde(rename = "podIP")]
    pub pod_ip: Option<String>,
    /// Current service state of pod.
    pub conditions: HashMap<PodConditionType, PodCondition>,
    /// The list has one entry per container in the manifest.
    /// Each entry is currently the output of docker inspect.
    pub container_statuses: Vec<ContainerStatus>,
}

impl Default for PodStatus {
    fn default() -> Self {
        let mut conditions = HashMap::new();
        PodConditionType::iter().for_each(|c| {
            conditions.insert(c, PodCondition::default());
        });
        PodStatus {
            host_ip: None,
            start_time: Local::now().naive_utc(),
            phase: PodPhase::Pending,
            pod_ip: None,
            conditions,
            container_statuses: vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PodPhase {
    /// All containers in the pod have terminated,
    /// and at least one container has terminated in failure.
    /// The container either exited with non-zero status
    /// or was terminated by the system.
    Failed,
    /// The pod has been accepted by the Kubernetes system,
    /// but one or more of the container images has not been created.
    /// This includes time before being scheduled
    /// as well as time spent downloading images over the network,
    /// which could take a while.
    Pending,
    /// The pod has been bound to a node,
    /// and all of the containers have been created.
    /// At least one container is still running,
    /// or is in the process of starting or restarting.
    Running,
    /// All containers in the pod have terminated in success,
    /// and will not be restarted.
    Succeeded,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, EnumIter, Clone)]
pub enum PodConditionType {
    /// All containers in the pod are ready.
    ContainersReady,
    /// All init containers have completed successfully.
    Initialized,
    /// The pod has been scheduled to a node.
    PodScheduled,
    /// The pod is able to serve requests
    /// and should be added to the load balancing pools of all matching Services.
    Ready,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct PodCondition {
    /// Status is the status of the condition.
    /// Can be True, False, Unknown.
    pub status: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ContainerState {
    Running,
    Terminated,
    Waiting,
}

impl From<Option<bollard::models::ContainerState>> for ContainerState {
    fn from(state: Option<bollard::models::ContainerState>) -> Self {
        match state {
            Some(state) => match state.status {
                Some(status) => match status {
                    ContainerStateStatusEnum::RUNNING => ContainerState::Running,
                    ContainerStateStatusEnum::EXITED | ContainerStateStatusEnum::DEAD => {
                        ContainerState::Terminated
                    },
                    _ => ContainerState::Waiting,
                },
                None => ContainerState::Waiting,
            },
            None => ContainerState::Waiting,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStatus {
    /// This must be a DNS_LABEL.
    /// Each container in a pod must have a unique name.
    /// Cannot be updated.
    pub name: String,
    /// The image the container is running.
    pub image: String,
    /// Container's ID
    pub container_id: String,
    /// State is the current state of the container.
    pub state: ContainerState,
}

impl From<ContainerInspectResponse> for ContainerStatus {
    fn from(response: ContainerInspectResponse) -> Self {
        ContainerStatus {
            name: response.name.expect("Container name not found"),
            image: response.image.expect("Container image not found"),
            container_id: response.id.expect("Container ID not found"),
            state: ContainerState::from(response.state),
        }
    }
}
