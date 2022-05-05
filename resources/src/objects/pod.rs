use std::{collections::HashMap, default::Default};

use bollard::models::{ContainerInspectResponse, ContainerStateStatusEnum};
use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, IntoEnumIterator};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pod {
    pub spec: PodSpec,
    pub status: Option<PodStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PodSpec {
    /// List of containers belonging to the pod.
    /// Containers cannot currently be added or removed.
    /// There must be at least one container in a Pod. Cannot be updated.
    pub containers: Vec<Container>,
    /// List of volumes that can be mounted by containers belonging to the pod.
    #[serde(default)]
    pub volumes: Vec<Volume>,
    /// Host networking requested for this pod.
    /// Use the host's network namespace.
    /// If this option is set, the ports that will be used must be specified.
    /// Default to false.
    #[serde(default)]
    pub host_network: bool,
    /// NodeName is a request to schedule this pod onto a specific node.
    /// If it is non-empty, the scheduler simply schedules this pod onto that node,
    /// assuming that it fits resource requirements.
    pub node_name: Option<String>,
}

impl PodSpec {
    pub fn network_mode(&self) -> String {
        if self.host_network {
            "host".to_string()
        } else {
            "bridge".to_string()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    /// Name of the container specified as a DNS_LABEL.
    /// Each container in a pod must have a unique name (DNS_LABEL).
    /// Cannot be updated.
    pub name: String,
    /// Docker image name.
    pub image: String,
    /// Entrypoint array. Not executed within a shell.
    /// The docker image's ENTRYPOINT is used if this is not provided.
    #[serde(default)]
    pub command: Vec<String>,
    /// List of ports to expose from the container.
    pub ports: Vec<ContainerPort>,
    /// Pod volumes to mount into the container's filesystem.
    /// Cannot be updated.
    #[serde(default)]
    pub volume_mounts: Vec<VolumeMount>,
    /// Compute Resources required by this container. Cannot be updated.
    #[serde(default)]
    pub resources: ResourceRequirements,
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
pub struct VolumeMount {
    /// Path within the container at which the volume should be mounted.
    pub mount_path: String,
    /// This must match the Name of a Volume.
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ResourceRequirements {
    /// Limits describes the maximum amount of compute resources allowed.
    pub limits: Resource,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Resource {
    /// An integer value representing this container's
    /// relative CPU weight versus other containers. Defaults to 100.
    pub cpu: i64,
    /// Memory limit in bytes. Defaults to 0.
    pub memory: i64,
}

impl Default for Resource {
    fn default() -> Self {
        Resource {
            cpu: 100,
            memory: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Volume {
    /// Volume's name.
    /// Must be a DNS_LABEL and unique within the pod.
    pub name: String,
    #[serde(flatten)]
    pub config: VolumeConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum VolumeConfig {
    /// HostPath represents a pre-existing file
    /// or directory on the host machine
    /// that is directly exposed to the container.
    ///
    /// path (String): Path of the directory on the host.
    /// If the path is a symlink, it will follow the link to the real path.
    HostPath(String),
    /// EmptyDir represents a temporary directory that shares a pod's lifetime.
    EmptyDir(()),
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

#[derive(Debug, Serialize, Deserialize, Clone, Display)]
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
